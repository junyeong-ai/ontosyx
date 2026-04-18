-- Migration 0007 — Phase A: i18n + IR evolution
--
-- Aligns the persisted JSONB ontology shape with ox-core's post-Phase-A types:
--
-- 1. Adds `workspaces.primary_locale` (BCP 47) + `workspaces.locale_fallback`
--    (fallback chain) so workspace scope carries a canonical locale policy.
-- 2. Rewrites all ontology JSONB (design_projects.ontology, ontology_snapshots.ontology,
--    saved_ontologies.ontology_ir) so legacy `description: "..."` string fields
--    become `{"default":"...","translations":{}}` (LocalizedText shape) and
--    legacy `version: N` scalars become `{"number": N}` (OntologyVersion shape).
-- 3. Strips the removed `source_table` field from every node within the JSONB,
--    since NodeTypeDef no longer carries it (source_lineage is authoritative).
--
-- All transformations are idempotent: re-running the migration on already-
-- converted data is a no-op, so multi-step rollforward / repair workflows
-- stay safe.

BEGIN;

-- ============================================================================
-- 1. Workspace locale policy
-- ============================================================================

ALTER TABLE workspaces
    ADD COLUMN IF NOT EXISTS primary_locale TEXT NOT NULL DEFAULT 'ko';

ALTER TABLE workspaces
    ADD COLUMN IF NOT EXISTS locale_fallback JSONB NOT NULL DEFAULT '["ko","en"]'::jsonb;

-- BCP 47 tag shape guard. Matches `ko`, `en`, `en-us`, `zh-hant`, `zh-hant-tw`,
-- ... — 2–3 ASCII letter primary subtag plus 0+ additional subtags of 2–8
-- alphanumerics separated by hyphens. Normalised to lowercase by ox-core.
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'workspaces_primary_locale_check'
    ) THEN
        ALTER TABLE workspaces
            ADD CONSTRAINT workspaces_primary_locale_check
            CHECK (primary_locale ~ '^[a-z]{2,3}(-[a-z0-9]{2,8})*$');
    END IF;
END
$$;

-- The fallback chain must be a non-empty JSON array of strings matching the
-- same BCP 47 shape. Validation is done at jsonb level with a CHECK that
-- walks the elements. A PL/pgSQL function keeps the CHECK readable.
CREATE OR REPLACE FUNCTION fn_validate_locale_chain(chain jsonb) RETURNS boolean AS $$
DECLARE
    elem jsonb;
BEGIN
    IF jsonb_typeof(chain) <> 'array' THEN
        RETURN false;
    END IF;
    IF jsonb_array_length(chain) = 0 THEN
        RETURN false;
    END IF;
    FOR elem IN SELECT value FROM jsonb_array_elements(chain) LOOP
        IF jsonb_typeof(elem) <> 'string' THEN
            RETURN false;
        END IF;
        IF NOT (elem #>> '{}' ~ '^[a-z]{2,3}(-[a-z0-9]{2,8})*$') THEN
            RETURN false;
        END IF;
    END LOOP;
    RETURN true;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'workspaces_locale_fallback_check'
    ) THEN
        ALTER TABLE workspaces
            ADD CONSTRAINT workspaces_locale_fallback_check
            CHECK (fn_validate_locale_chain(locale_fallback));
    END IF;
END
$$;

-- ============================================================================
-- 2. JSONB shape migration helpers
-- ============================================================================

-- Promote a scalar description (string / null / missing) into the canonical
-- LocalizedText shape `{"default": "...", "translations": {}}`. Idempotent:
-- if the input is already a LocalizedText object, it is returned unchanged.
CREATE OR REPLACE FUNCTION fn_to_localized_text(v jsonb) RETURNS jsonb AS $$
BEGIN
    IF v IS NULL OR v = 'null'::jsonb THEN
        RETURN jsonb_build_object('default', '', 'translations', '{}'::jsonb);
    END IF;
    IF jsonb_typeof(v) = 'string' THEN
        RETURN jsonb_build_object(
            'default', v #>> '{}',
            'translations', '{}'::jsonb
        );
    END IF;
    IF jsonb_typeof(v) = 'object' THEN
        -- Already a LocalizedText (has default) or an empty object — normalise either way.
        RETURN jsonb_build_object(
            'default', COALESCE(v -> 'default', '""'::jsonb) #>> '{}',
            'translations', COALESCE(v -> 'translations', '{}'::jsonb)
        );
    END IF;
    -- Unsupported shape (number, array) → empty localized text, preserves invariant.
    RETURN jsonb_build_object('default', '', 'translations', '{}'::jsonb);
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Promote a scalar version (integer) into the canonical OntologyVersion shape
-- `{"number": N}`. Idempotent for objects.
CREATE OR REPLACE FUNCTION fn_to_ontology_version(v jsonb) RETURNS jsonb AS $$
BEGIN
    IF v IS NULL OR v = 'null'::jsonb THEN
        RETURN jsonb_build_object('number', 1);
    END IF;
    IF jsonb_typeof(v) = 'number' THEN
        RETURN jsonb_build_object('number', (v #>> '{}')::int);
    END IF;
    IF jsonb_typeof(v) = 'object' THEN
        RETURN v;
    END IF;
    RETURN jsonb_build_object('number', 1);
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Rewrite a single OntologyIR document in place:
-- - Top-level description → LocalizedText
-- - Top-level version → OntologyVersion
-- - Each node.description / edge.description / property.description → LocalizedText
-- - Strip node.source_table (field removed in Phase A Day 3)
-- - Property-level description conversion walks node.properties and edge.properties
CREATE OR REPLACE FUNCTION fn_migrate_ontology(ont jsonb) RETURNS jsonb AS $$
DECLARE
    out_ont jsonb;
    out_nodes jsonb := '[]'::jsonb;
    out_edges jsonb := '[]'::jsonb;
    node jsonb;
    edge jsonb;
    out_props jsonb;
    prop jsonb;
BEGIN
    IF ont IS NULL THEN
        RETURN NULL;
    END IF;
    IF jsonb_typeof(ont) <> 'object' THEN
        RETURN ont;
    END IF;

    out_ont := ont;
    out_ont := jsonb_set(out_ont, '{description}', fn_to_localized_text(ont -> 'description'), true);
    out_ont := jsonb_set(out_ont, '{version}', fn_to_ontology_version(ont -> 'version'), true);

    -- Node types
    IF ont ? 'node_types' THEN
        FOR node IN SELECT value FROM jsonb_array_elements(ont -> 'node_types') LOOP
            node := jsonb_set(node, '{description}', fn_to_localized_text(node -> 'description'), true);
            -- Remove legacy field
            node := node - 'source_table';
            -- Walk properties
            IF node ? 'properties' THEN
                out_props := '[]'::jsonb;
                FOR prop IN SELECT value FROM jsonb_array_elements(node -> 'properties') LOOP
                    prop := jsonb_set(prop, '{description}', fn_to_localized_text(prop -> 'description'), true);
                    out_props := out_props || jsonb_build_array(prop);
                END LOOP;
                node := jsonb_set(node, '{properties}', out_props, true);
            END IF;
            out_nodes := out_nodes || jsonb_build_array(node);
        END LOOP;
        out_ont := jsonb_set(out_ont, '{node_types}', out_nodes, true);
    END IF;

    -- Edge types
    IF ont ? 'edge_types' THEN
        FOR edge IN SELECT value FROM jsonb_array_elements(ont -> 'edge_types') LOOP
            edge := jsonb_set(edge, '{description}', fn_to_localized_text(edge -> 'description'), true);
            IF edge ? 'properties' THEN
                out_props := '[]'::jsonb;
                FOR prop IN SELECT value FROM jsonb_array_elements(edge -> 'properties') LOOP
                    prop := jsonb_set(prop, '{description}', fn_to_localized_text(prop -> 'description'), true);
                    out_props := out_props || jsonb_build_array(prop);
                END LOOP;
                edge := jsonb_set(edge, '{properties}', out_props, true);
            END IF;
            out_edges := out_edges || jsonb_build_array(edge);
        END LOOP;
        out_ont := jsonb_set(out_ont, '{edge_types}', out_edges, true);
    END IF;

    RETURN out_ont;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- ============================================================================
-- 3. Apply migration to persisted ontology columns
-- ============================================================================

-- design_projects.ontology (nullable — only update when populated)
UPDATE design_projects
   SET ontology = fn_migrate_ontology(ontology)
 WHERE ontology IS NOT NULL;

-- ontology_snapshots.ontology (NOT NULL)
UPDATE ontology_snapshots
   SET ontology = fn_migrate_ontology(ontology);

-- saved_ontologies.ontology_ir (NOT NULL)
UPDATE saved_ontologies
   SET ontology_ir = fn_migrate_ontology(ontology_ir);

-- ============================================================================
-- 4. Drop helper functions (they served their migration purpose)
-- ============================================================================
--
-- `fn_to_localized_text`, `fn_to_ontology_version`, `fn_migrate_ontology` are
-- migration-time only. Keeping them around could mislead future contributors
-- into thinking the canonical shape is still plastic. `fn_validate_locale_chain`
-- stays — it is referenced by the workspace CHECK constraint.
DROP FUNCTION IF EXISTS fn_migrate_ontology(jsonb);
DROP FUNCTION IF EXISTS fn_to_ontology_version(jsonb);
DROP FUNCTION IF EXISTS fn_to_localized_text(jsonb);

COMMIT;

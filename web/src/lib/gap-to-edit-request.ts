import type { QualityGap } from "@/types/api";

/**
 * Convert a quality gap into a natural-language edit request
 * suitable for the `editProject` API endpoint.
 */
export function gapToEditRequest(gap: QualityGap): string {
  const loc = gap.location;

  switch (gap.category) {
    case "missing_description": {
      if (loc.ref_type === "node") {
        return `Add a meaningful description to the node '${loc.label}'`;
      }
      if (loc.ref_type === "node_property") {
        return `Add a description to property '${loc.property_name}' on node '${loc.label}'`;
      }
      if (loc.ref_type === "edge") {
        return `Add a meaningful description to the edge '${loc.label}'`;
      }
      if (loc.ref_type === "edge_property") {
        return `Add a description to property '${loc.property_name}' on edge '${loc.label}'`;
      }
      break;
    }

    case "opaque_enum_value": {
      if (loc.ref_type === "node_property") {
        return `Describe what each value means for property '${loc.property_name}' on '${loc.label}'`;
      }
      if (loc.ref_type === "edge_property") {
        return `Describe what each value means for property '${loc.property_name}' on edge '${loc.label}'`;
      }
      break;
    }

    case "missing_foreign_key_edge": {
      if (loc.ref_type === "source_foreign_key") {
        return `Add an edge for the foreign key relationship between '${loc.from_table}' and '${loc.to_table}'`;
      }
      break;
    }

    case "unmapped_source_column": {
      if (loc.ref_type === "source_column") {
        return `Add a property mapping for column '${loc.column}' from table '${loc.table}'`;
      }
      break;
    }

    case "single_value_bias": {
      if (loc.ref_type === "node_property") {
        return `Review and improve property '${loc.property_name}' on '${loc.label}' which has low value diversity`;
      }
      if (loc.ref_type === "edge_property") {
        return `Review and improve property '${loc.property_name}' on edge '${loc.label}' which has low value diversity`;
      }
      break;
    }

    case "small_sample":
      return "Review the ontology structure given the small sample size";

    case "numeric_enum_code": {
      if (loc.ref_type === "node_property" || loc.ref_type === "edge_property") {
        return `Property '${loc.property_name}' on '${loc.label}' contains numeric codes that may represent categories. Add a description explaining what each code means`;
      }
      break;
    }

    case "sparse_property": {
      if (loc.ref_type === "node_property" || loc.ref_type === "edge_property") {
        return `Review property '${loc.property_name}' on '${loc.label}' which is mostly empty. Consider making it nullable or removing it if not needed`;
      }
      break;
    }

    case "unmapped_source_table": {
      if (loc.ref_type === "source_table") {
        return `Add a node type to represent the source table '${loc.table}' which has no corresponding entity in the ontology`;
      }
      break;
    }

    case "missing_containment_edge": {
      if (loc.ref_type === "source_foreign_key") {
        return `Add a containment edge for the relationship between '${loc.from_table}' and '${loc.to_table}'`;
      }
      break;
    }

    case "duplicate_edge": {
      if (loc.ref_type === "node") {
        return `Review edges connected to '${loc.label}' and remove any semantically duplicate edges that represent the same relationship`;
      }
      break;
    }

    case "orphan_node": {
      if (loc.ref_type === "node") {
        return `Node '${loc.label}' has no edges connecting it to the rest of the graph. Add edges to connect it or remove it if unnecessary`;
      }
      break;
    }

    case "hub_node": {
      if (loc.ref_type === "node") {
        return `Node '${loc.label}' has too many edges. Consider splitting it into more focused node types to reduce complexity`;
      }
      break;
    }

    case "property_type_inconsistency": {
      return `Property '${loc.ref_type === "node" ? loc.label : ""}' has inconsistent types across node types. Unify to a single type for consistency`;
    }

    case "overloaded_property": {
      return `Property appears on too many node types. Consider extracting it into a dedicated node type`;
    }

    case "self_referential_edge": {
      if (loc.ref_type === "edge") {
        return `Edge '${loc.label}' is self-referential. Verify this recursive relationship is intentional`;
      }
      break;
    }
  }

  return `Fix the following quality issue: ${gap.issue}`;
}

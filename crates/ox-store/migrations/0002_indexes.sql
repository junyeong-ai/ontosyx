-- 0002_indexes.sql: All indexes (consolidated from pg_dump)

-- ============================================================================
-- acl_policies
-- ============================================================================
CREATE INDEX idx_acl_priority ON acl_policies USING btree (workspace_id, priority DESC);
CREATE INDEX idx_acl_resource ON acl_policies USING btree (resource_type, resource_value);
CREATE INDEX idx_acl_subject ON acl_policies USING btree (subject_type, subject_value);
CREATE INDEX idx_acl_workspace ON acl_policies USING btree (workspace_id, is_active);

-- ============================================================================
-- agent_events
-- ============================================================================
CREATE INDEX idx_agent_events_session ON agent_events USING btree (session_id, sequence);
CREATE INDEX idx_agent_events_ws ON agent_events USING btree (workspace_id, session_id);

-- ============================================================================
-- agent_sessions
-- ============================================================================
CREATE INDEX idx_agent_sessions_completed ON agent_sessions USING btree (completed_at) WHERE (completed_at IS NOT NULL);
CREATE INDEX idx_agent_sessions_user ON agent_sessions USING btree (user_id, created_at DESC);
CREATE INDEX idx_sessions_workspace ON agent_sessions USING btree (workspace_id, created_at DESC);
CREATE UNIQUE INDEX uq_agent_sessions_ws_id ON agent_sessions USING btree (workspace_id, id);

-- ============================================================================
-- analysis_recipes
-- ============================================================================
CREATE INDEX idx_recipes_algorithm_type ON analysis_recipes USING btree (algorithm_type);
CREATE INDEX idx_recipes_parent ON analysis_recipes USING btree (parent_id) WHERE (parent_id IS NOT NULL);
CREATE INDEX idx_recipes_status ON analysis_recipes USING btree (status);
CREATE INDEX idx_recipes_workspace ON analysis_recipes USING btree (workspace_id, created_at DESC);
CREATE UNIQUE INDEX uq_analysis_recipes_ws_id ON analysis_recipes USING btree (workspace_id, id);

-- ============================================================================
-- analysis_results
-- ============================================================================
CREATE INDEX idx_analysis_results_cache ON analysis_results USING btree (input_hash, recipe_id);
CREATE INDEX idx_analysis_results_recipe ON analysis_results USING btree (recipe_id, created_at DESC);
CREATE INDEX idx_analysis_results_recipe_created ON analysis_results USING btree (recipe_id, created_at DESC);
CREATE INDEX idx_results_workspace ON analysis_results USING btree (workspace_id, created_at DESC);

-- ============================================================================
-- approval_requests
-- ============================================================================
CREATE INDEX idx_approval_expires ON approval_requests USING btree (expires_at) WHERE (status = 'pending');
CREATE INDEX idx_approval_requester ON approval_requests USING btree (requester_id, created_at DESC);
CREATE INDEX idx_approval_resource ON approval_requests USING btree (resource_type, resource_id);
CREATE INDEX idx_approval_workspace_status ON approval_requests USING btree (workspace_id, status, created_at DESC);

-- ============================================================================
-- audit_log
-- ============================================================================
CREATE INDEX idx_audit_log_action ON audit_log USING btree (action, created_at DESC);
CREATE INDEX idx_audit_log_resource ON audit_log USING btree (resource_type, resource_id);
CREATE INDEX idx_audit_log_user ON audit_log USING btree (user_id, created_at DESC);
CREATE INDEX idx_audit_log_workspace ON audit_log USING btree (workspace_id, created_at DESC);

-- ============================================================================
-- dashboard_widgets
-- ============================================================================
CREATE INDEX idx_dashboard_widgets_ws ON dashboard_widgets USING btree (workspace_id, dashboard_id);
CREATE INDEX idx_widgets_dashboard ON dashboard_widgets USING btree (dashboard_id);

-- ============================================================================
-- dashboards
-- ============================================================================
CREATE INDEX idx_dashboards_public ON dashboards USING btree (updated_at DESC) WHERE (is_public = true);
CREATE INDEX idx_dashboards_share_token ON dashboards USING btree (share_token) WHERE (share_token IS NOT NULL);
CREATE INDEX idx_dashboards_user ON dashboards USING btree (user_id, updated_at DESC);
CREATE INDEX idx_dashboards_workspace ON dashboards USING btree (workspace_id, updated_at DESC);
CREATE UNIQUE INDEX uq_dashboards_ws_id ON dashboards USING btree (workspace_id, id);

-- ============================================================================
-- data_lineage
-- ============================================================================
CREATE INDEX idx_lineage_label ON data_lineage USING btree (graph_label, graph_element_type);
CREATE INDEX idx_lineage_project ON data_lineage USING btree (project_id) WHERE (project_id IS NOT NULL);
CREATE INDEX idx_lineage_source ON data_lineage USING btree (source_name, source_table);
CREATE INDEX idx_lineage_workspace ON data_lineage USING btree (workspace_id, started_at DESC);

-- ============================================================================
-- design_projects
-- ============================================================================
CREATE INDEX idx_design_projects_status ON design_projects USING btree (status) WHERE (archived_at IS NULL);
CREATE INDEX idx_design_projects_updated_at_id ON design_projects USING btree (updated_at DESC, id DESC) WHERE (archived_at IS NULL);
CREATE INDEX idx_design_projects_user ON design_projects USING btree (user_id, updated_at DESC);
CREATE INDEX idx_projects_archived ON design_projects USING btree (archived_at) WHERE (archived_at IS NOT NULL);
CREATE INDEX idx_projects_workspace ON design_projects USING btree (workspace_id, created_at DESC);
CREATE UNIQUE INDEX uq_design_projects_ws_id ON design_projects USING btree (workspace_id, id);

-- ============================================================================
-- knowledge_entries
-- ============================================================================
CREATE INDEX idx_knowledge_affected_labels ON knowledge_entries USING gin (affected_labels);
CREATE INDEX idx_knowledge_affected_properties ON knowledge_entries USING gin (affected_properties);
CREATE INDEX idx_knowledge_confidence ON knowledge_entries USING btree (workspace_id, confidence DESC) WHERE (status::text = 'approved');
CREATE INDEX idx_knowledge_ontology_active ON knowledge_entries USING btree (workspace_id, ontology_name, status) WHERE (status::text = ANY (ARRAY['approved', 'draft']));
CREATE INDEX idx_knowledge_workspace ON knowledge_entries USING btree (workspace_id);
CREATE UNIQUE INDEX uq_knowledge_content_hash ON knowledge_entries USING btree (workspace_id, ontology_name, content_hash);
CREATE UNIQUE INDEX uq_knowledge_ws_id ON knowledge_entries USING btree (workspace_id, id);

-- ============================================================================
-- memory_entries
-- ============================================================================
CREATE INDEX idx_memory_content_trgm ON memory_entries USING gin (content gin_trgm_ops);
CREATE INDEX idx_memory_embedding ON memory_entries USING hnsw (embedding vector_cosine_ops);
CREATE INDEX idx_memory_last_accessed ON memory_entries USING btree (last_accessed_at) WHERE (last_accessed_at IS NOT NULL);
CREATE INDEX idx_memory_metadata_ontology ON memory_entries USING btree ((metadata ->> 'ontology_id'));
CREATE INDEX idx_memory_source ON memory_entries USING btree ((metadata ->> 'source'));
CREATE INDEX idx_memory_workspace ON memory_entries USING btree (workspace_id);

-- ============================================================================
-- model_configs
-- ============================================================================
CREATE UNIQUE INDEX idx_model_configs_scope_name ON model_configs USING btree (COALESCE(workspace_id, '00000000-0000-0000-0000-000000000000'::uuid), name);

-- ============================================================================
-- model_routing_rules
-- ============================================================================
CREATE INDEX idx_routing_lookup ON model_routing_rules USING btree (COALESCE(workspace_id, '00000000-0000-0000-0000-000000000000'::uuid), operation, priority DESC);

-- ============================================================================
-- notification_channels
-- ============================================================================
CREATE INDEX idx_notification_channels_workspace ON notification_channels USING btree (workspace_id);

-- ============================================================================
-- notification_log
-- ============================================================================
CREATE INDEX idx_notification_log_channel ON notification_log USING btree (channel_id);
CREATE INDEX idx_notification_log_workspace ON notification_log USING btree (workspace_id, created_at DESC);

-- ============================================================================
-- ontology_snapshots
-- ============================================================================
CREATE INDEX idx_ontology_snapshots_project ON ontology_snapshots USING btree (project_id, revision DESC);
CREATE INDEX idx_ontology_snapshots_ws ON ontology_snapshots USING btree (workspace_id, project_id);

-- ============================================================================
-- ontology_verifications
-- ============================================================================
CREATE UNIQUE INDEX idx_verifications_active ON ontology_verifications USING btree (ontology_id, element_id, verified_by) WHERE (invalidated_at IS NULL);
CREATE INDEX idx_verifications_ontology ON ontology_verifications USING btree (ontology_id) WHERE (invalidated_at IS NULL);
CREATE INDEX idx_verifications_workspace ON ontology_verifications USING btree (workspace_id);

-- ============================================================================
-- pending_embeddings
-- ============================================================================
CREATE INDEX idx_pending_embeddings_retry ON pending_embeddings USING btree (retry_count, created_at) WHERE (retry_count < 3);

-- ============================================================================
-- pinboard_items
-- ============================================================================
CREATE INDEX idx_pinboard_user ON pinboard_items USING btree (user_id, pinned_at DESC, id DESC);
CREATE INDEX idx_pins_workspace ON pinboard_items USING btree (workspace_id);

-- ============================================================================
-- prompt_templates
-- ============================================================================
CREATE INDEX idx_prompt_templates_active ON prompt_templates USING btree (name, is_active) WHERE (is_active = true);
CREATE INDEX idx_templates_workspace ON prompt_templates USING btree (workspace_id);

-- ============================================================================
-- quality_results
-- ============================================================================
CREATE INDEX idx_quality_results_rule ON quality_results USING btree (rule_id, evaluated_at DESC);
CREATE INDEX idx_quality_results_workspace ON quality_results USING btree (workspace_id, evaluated_at DESC);

-- ============================================================================
-- quality_rules
-- ============================================================================
CREATE INDEX idx_quality_rules_active ON quality_rules USING btree (is_active, severity);
CREATE INDEX idx_quality_rules_label ON quality_rules USING btree (target_label);
CREATE INDEX idx_quality_rules_workspace ON quality_rules USING btree (workspace_id);
CREATE UNIQUE INDEX uq_quality_rules_ws_id ON quality_rules USING btree (workspace_id, id);

-- ============================================================================
-- query_executions
-- ============================================================================
CREATE INDEX idx_queries_workspace ON query_executions USING btree (workspace_id, created_at DESC);
CREATE INDEX idx_query_executions_ontology_ref ON query_executions USING btree (saved_ontology_id) WHERE (saved_ontology_id IS NOT NULL);
CREATE INDEX idx_query_executions_user ON query_executions USING btree (user_id, created_at DESC);
CREATE UNIQUE INDEX uq_query_executions_ws_id ON query_executions USING btree (workspace_id, id);

-- ============================================================================
-- saved_ontologies
-- ============================================================================
CREATE INDEX idx_ontologies_workspace ON saved_ontologies USING btree (workspace_id, created_at DESC);
CREATE UNIQUE INDEX uq_saved_ontologies_ws_id ON saved_ontologies USING btree (workspace_id, id);

-- ============================================================================
-- saved_reports
-- ============================================================================
CREATE INDEX idx_reports_workspace ON saved_reports USING btree (workspace_id, updated_at DESC);
CREATE INDEX idx_saved_reports_ontology ON saved_reports USING btree (ontology_id);
CREATE INDEX idx_saved_reports_public ON saved_reports USING btree (updated_at DESC) WHERE (is_public = true);
CREATE INDEX idx_saved_reports_user ON saved_reports USING btree (user_id, updated_at DESC);

-- ============================================================================
-- scheduled_tasks
-- ============================================================================
CREATE INDEX idx_schedtasks_workspace ON scheduled_tasks USING btree (workspace_id);
CREATE INDEX idx_scheduled_tasks_next_run ON scheduled_tasks USING btree (next_run_at) WHERE (enabled = true);
CREATE INDEX idx_scheduled_tasks_recipe ON scheduled_tasks USING btree (recipe_id);

-- ============================================================================
-- tool_approvals
-- ============================================================================
CREATE INDEX idx_tool_approvals_session ON tool_approvals USING btree (session_id, created_at DESC);
CREATE INDEX idx_tool_approvals_ws ON tool_approvals USING btree (workspace_id, session_id);

-- ============================================================================
-- usage_records
-- ============================================================================
CREATE INDEX idx_usage_resource_type ON usage_records USING btree (resource_type, created_at DESC);
CREATE INDEX idx_usage_user_time ON usage_records USING btree (user_id, created_at DESC);
CREATE INDEX idx_usage_workspace_time ON usage_records USING btree (workspace_id, created_at DESC);

-- ============================================================================
-- workbench_perspectives
-- ============================================================================
CREATE INDEX idx_perspectives_topology ON workbench_perspectives USING btree (user_id, topology_signature);
CREATE UNIQUE INDEX idx_perspectives_unique_default ON workbench_perspectives USING btree (user_id, lineage_id) WHERE (is_default = true);
CREATE INDEX idx_perspectives_user_lineage ON workbench_perspectives USING btree (user_id, lineage_id);
CREATE INDEX idx_workbench_perspectives_ws ON workbench_perspectives USING btree (workspace_id);

-- ============================================================================
-- workspace_members
-- ============================================================================
CREATE INDEX idx_workspace_members_user ON workspace_members USING btree (user_id);

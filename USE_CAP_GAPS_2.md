# Use Capability Gaps 2

This file tracks a second-pass synthetic inventory of realistic programmatic
tool-call workloads related to the gallery added in commit `9cbcca8`.

Unlike `USE_CASE_GAPS.md`, this is not an executable pass/fail audit. It is a
curated backlog of additional realistic examples that should either:

- become future audited gallery cases once the runtime and host contract can
  support them cleanly
- or become explicit fail-closed examples when they require semantics that
  `mustard` should intentionally avoid

Current synthesized summary on April 11, 2026:

- total generated examples: `112`
- subagent passes used: `7`
- current audited gallery baseline from `USE_CASE_GAPS.md`: `24` passing
  examples
- strongest recurring gap clusters: `advanced_dates`, `document_diff`,
  `binary_payloads`, `streaming_io`, `multi_party_state`,
  `schema_heavy_inputs`, `geospatial_math`, `long_running_job`,
  `human_approval`

## Generation Passes

- `gpt-5.4` with `xhigh`: finance, risk, and pricing workloads
- `gpt-5.4-mini` with `medium`: operations, SRE, and infra workloads
- `gpt-5.3-codex` with `high`: compliance, security, legal, and governance
  workflows
- `gpt-5.2` with `medium`: developer productivity, CI, and release workflows
- `gpt-5.4-mini` with `high`: supply chain, marketplace, trust and safety, and
  logistics workflows
- `gpt-5.3-codex` with `medium`: customer support, sales ops, success, and
  account management workflows
- `gpt-5.4-mini` with `high`: healthcare, public sector, research, education,
  and other regulated operational workflows

## Current Status

There are no audited runtime failures recorded in this file yet. The current
value is coverage pressure:

- many examples below appear to be coverage-only expansions that fit the
  existing explicit-capability model once concrete host stubs exist
- the highest-risk future additions are the cases that depend on document/image
  ingestion, scanned attachments, live streaming feeds, dense multi-party state,
  settlement calendars, or geospatial computations
- if these workloads are promoted into the executable gallery, keep the
  workloads realistic and record either the true runtime bug or the deliberate
  fail-closed boundary instead of simplifying the example to fit the runtime

## Highest-Signal Gap Clusters

- `coverage_only`: realistic new audited examples that mostly need host stubs
  and catalog breadth rather than new guest semantics
- `document_diff` and `binary_payloads`: contract packets, scanned annexes,
  PDFs, session replays, and attachment-heavy workflows
- `streaming_io` and `long_running_job`: event-window monitoring and larger
  scenario sweeps that stress resume or chunking strategy
- `advanced_dates` and `geospatial_math`: holiday calendars, settlement
  cutoffs, catastrophe tiles, and other non-trivial temporal or spatial logic
- `multi_party_state` and `schema_heavy_inputs`: netting sets, payout pools,
  structured products, dense CPQ payloads, and other nested cross-entity data
- `human_approval`: workflows that are still explicit-capability friendly but
  want richer multi-step review and resume patterns

## Generated Inventory

### Finance, Risk, and Pricing

- `finance_intraday_liquidity_ladder`: Intraday Liquidity Stress Ladder.
  Surface: `fetch_cash_positions`, `fetch_settlement_windows`,
  `fetch_credit_lines`, `score_shortfall`, `plan_draws`. Shape:
  `fan_out_reduce`. Cluster: `advanced_dates`. Treasury laddering across
  settlement cutoffs and emergency facilities.
- `risk_margin_call_netting`: Margin Call Netting Review. Surface:
  `fetch_trades`, `fetch_collateral`, `fetch_margin_rules`,
  `score_net_exposure`, `issue_call`. Shape: `netting_reduce_issue`. Cluster:
  `multi_party_state`. Cross-counterparty netting and collateral waterfalls.
- `pricing_deposit_elasticity_refresh`: Deposit Rate Elasticity Refresh.
  Surface: `fetch_rate_history`, `fetch_balance_flows`,
  `fetch_competitor_rates`, `score_elasticity`, `plan_rate_moves`. Shape:
  `timeseries_reduce_plan`. Cluster: `advanced_dates`. Bounded repricing from
  recent migration and market signals.
- `finance_commission_leakage_audit`: Broker Commission Leakage Audit. Surface:
  `list_broker_statements`, `fetch_trade_allocations`,
  `score_commission_variance`, `issue_recovery_cases`. Shape:
  `reconcile_reduce_issue`. Cluster: `schema_heavy_inputs`. Dense
  trade-allocation statement reconciliation.
- `risk_commercial_covenant_triage`: Commercial Covenant Breach Triage.
  Surface: `fetch_borrower_financials`, `fetch_covenant_pack`,
  `review_waivers`, `score_breach_risk`, `issue_watchlist`. Shape:
  `diff_reduce_route`. Cluster: `document_diff`. Amended covenant definitions
  versus quarterly results.
- `pricing_secondary_price_exception`: Secondary Price Exception Review.
  Surface: `fetch_trade_request`, `fetch_reference_curves`,
  `fetch_inventory_limits`, `score_price_outlier`, `review_exception`. Shape:
  `score_pause_resume`. Cluster: `human_approval`. Dealer override routing with
  explicit guardrails.
- `finance_revrec_carveout_review`: Revenue Recognition Carve-out Review.
  Surface: `fetch_contract_version`, `fetch_delivery_events`,
  `review_revrec_rules`, `score_exception_risk`, `issue_memo`. Shape:
  `diff_reduce_issue`. Cluster: `document_diff`. Contract language changes that
  shift revenue timing.
- `risk_merchant_reserve_adjustment`: Merchant Reserve Adjustment Proposal.
  Surface: `fetch_merchant_exposure`, `fetch_chargeback_trends`,
  `fetch_cashflow_signals`, `score_reserve_need`, `plan_holdback`. Shape:
  `score_pause_resume`. Cluster: `human_approval`. Reserve resizing before risk
  committee approval.
- `pricing_cat_rate_cell_sweep`: Catastrophe Rate Cell Sweep. Surface:
  `fetch_zip_exposure`, `fetch_hazard_tiles`, `fetch_loss_history`,
  `score_rate_adequacy`, `review_cells`. Shape: `geospatial_reduce_review`.
  Cluster: `geospatial_math`. Hazard geographies and localized loss cells.
- `finance_factoring_advance_rate`: Invoice Factoring Advance-Rate Review.
  Surface: `fetch_invoice_pool`, `fetch_debtor_concentration`,
  `fetch_payment_history`, `score_dilution_risk`, `issue_offer`. Shape:
  `pool_reduce_issue`. Cluster: `multi_party_state`. Debtor overlap, dilution,
  and aging concentration.
- `risk_structured_note_sanity`: Structured Note Payoff Sanity Check. Surface:
  `fetch_term_sheet`, `fetch_market_fixings`, `fetch_payoff_grid`,
  `review_barrier_events`, `issue_exception`. Shape:
  `scenario_reduce_review`. Cluster: `schema_heavy_inputs`. Complex payoff
  states before valuation publication.
- `analytics_promo_margin_monitor`: Promo Margin Guardrail Monitor. Surface:
  `fetch_live_baskets`, `fetch_cost_bases`, `score_margin_floor`,
  `issue_pause`, `plan_reprice`. Shape: `stream_score_gate`. Cluster:
  `streaming_io`. Live basket snapshots driving repricing or pause decisions.
- `risk_collateral_annex_dispute`: Collateral Annex Dispute Prep. Surface:
  `fetch_annex_images`, `fetch_trade_disputes`, `review_clause_extracts`,
  `issue_dispute_packet`. Shape: `extract_review_issue`. Cluster:
  `binary_payloads`. Scanned annexes and dispute attachments.
- `finance_transfer_pricing_outliers`: Transfer Pricing Outlier Review.
  Surface: `fetch_entity_results`, `fetch_comparable_ranges`,
  `review_tax_policy`, `score_outliers`, `issue_adjustments`. Shape:
  `fan_out_reduce`. Cluster: `schema_heavy_inputs`. Entity-level deviations
  from policy and comparables.
- `pricing_reinsurance_attachment_reprice`: Reinsurance Attachment Reprice.
  Surface: `fetch_loss_triangles`, `fetch_layer_terms`, `fetch_quote_grid`,
  `review_capacity_quotes`, `plan_rebind`. Shape: `scenario_batch_plan`.
  Cluster: `long_running_job`. Large treaty-layer sweeps before negotiation.
- `finance_fx_hedge_rollover`: FX Hedge Rollover Planner. Surface:
  `fetch_exposure_forecast`, `fetch_forward_curve`, `fetch_hedge_book`,
  `score_rollover_gap`, `plan_rolls`. Shape: `curve_reduce_plan`. Cluster:
  `advanced_dates`. Month-end and holiday-calendar alignment for hedge rolls.

### Operations, SRE, and Infra

- `op-01`: Orphaned Load Balancers. Surface: `cloud_inventory`, `tag_lookup`,
  `state_diff`. Shape: `fan_out_reduce`. Cluster:
  `orphaned_resource_cluster`. Untagged or unattached load balancers across
  accounts.
- `op-02`: Certificate Expiry Audit. Surface: `cert_scan`, `chain_verify`,
  `san_check`. Shape: `plan_then_verify`. Cluster: `cert_hygiene`. Expiry
  windows, chain breaks, and hostname mismatches.
- `op-03`: DNS Drift Cleanse. Surface: `dns_query`, `zone_diff`,
  `ownership_map`. Shape: `reconcile_then_patch`. Cluster: `dns_hygiene`.
  Stale records and safe correction drafts.
- `op-04`: IAM Scope Review. Surface: `iam_inventory`, `policy_diff`,
  `access_graph`. Shape: `analyze_then_summarize`. Cluster: `access_drift`.
  Effective permissions versus baseline roles.
- `op-05`: Backup Policy Conformance. Surface: `backup_catalog`,
  `retention_check`, `policy_map`. Shape: `audit_then_report`. Cluster:
  `backup_posture`. Backup jobs and retention coverage against policy.
- `op-06`: Log Schema Drift. Surface: `log_sample`, `schema_parse`,
  `pipeline_map`. Shape: `fan_out_reduce`. Cluster: `telemetry_contract`.
  Breaking field changes before downstream corruption.
- `op-07`: Cron Overlap Detector. Surface: `scheduler_scan`, `job_graph`,
  `window_check`. Shape: `plan_then_verify`. Cluster:
  `automation_collisions`. Overlapping scheduled jobs contending for the same
  resources.
- `op-08`: Quota Fragmentation Review. Surface: `quota_read`,
  `namespace_scan`, `usage_rollup`. Shape: `reconcile_then_patch`. Cluster:
  `capacity_slicing`. Imbalanced resource quotas across tenants or namespaces.
- `op-09`: Secret Rotation Queue. Surface: `secret_inventory`, `age_filter`,
  `owner_lookup`. Shape: `plan_then_verify`. Cluster:
  `rotation_readiness`. Stale secrets grouped by owner and blast radius.
- `op-10`: Image Provenance Audit. Surface: `image_scan`, `sbom_lookup`,
  `signature_check`. Shape: `fan_out_reduce`. Cluster:
  `supply_chain_trust`. Runtime images versus provenance and signatures.
- `op-11`: Runbook Preflight. Surface: `cmd_preview`, `dependency_check`,
  `approval_gate`. Shape: `suspend_for_approval`. Cluster:
  `maintenance_safety`. Bounded validation before risky maintenance actions.
- `op-12`: Node Evidence Capture. Surface: `process_snapshot`, `file_hash`,
  `netstat_dump`. Shape: `suspend_for_approval`. Cluster:
  `forensic_capture`. Narrow evidence collection from suspect hosts.
- `op-13`: Alert Rule Pruning. Surface: `alert_histogram`, `signal_cluster`,
  `route_map`. Shape: `analyze_then_summarize`. Cluster: `alert_noise`.
  Low-signal rule consolidation opportunities.
- `op-14`: Owner Map Reconciliation. Surface: `repo_scan`, `ticket_link`,
  `user_map`. Shape: `reconcile_then_patch`. Cluster:
  `responsibility_drift`. Stale service ownership metadata across systems.
- `op-15`: Cost Leak Survey. Surface: `billing_slice`, `tag_audit`,
  `usage_rank`. Shape: `fan_out_reduce`. Cluster: `spend_leakage`. Persistent
  idle spend from low-value cloud assets.
- `op-16`: Incident Artifact Packaging. Surface: `log_bundle`,
  `timeline_merge`, `attachment_index`. Shape: `plan_then_verify`. Cluster:
  `evidence_packaging`. Reviewable incident bundles for handoff.

### Compliance, Security, Legal, and Governance

- `CMP-001`: Regulatory Change-to-Control Impact Mapper. Surface:
  `regulatory_feed`, `control_library`, `ticketing`, `cmdb`. Shape:
  `rules_mapping_with_owner_approval`. Cluster:
  `regulatory_ontology_mapping`. New rule text mapped to control owners.
- `PRV-002`: Data Residency Drift Remediation Orchestrator. Surface:
  `cloud_inventory`, `data_catalog`, `siem`, `itsm`. Shape:
  `hourly_drift_scan`. Cluster: `geo_data_lineage`. Unlawful storage locations
  routed with an evidentiary trail.
- `LGL-003`: Litigation Hold Custodian Acknowledgment Tracker. Surface:
  `hris`, `legal_hold_system`, `esign`, `email`. Shape:
  `hold_trigger_and_reminders`. Cluster:
  `defensible_acknowledgment_tracking`. Defensible notice completion records.
- `CMP-004`: SOX Evidence Freshness Sentinel. Surface: `grc`,
  `evidence_repository`, `chatops`. Shape: `scheduled_control_sweep`. Cluster:
  `evidence_freshness_automation`. Stale evidence before quarterly
  attestations.
- `LGL-005`: Open-Source License Obligation Resolver. Surface:
  `sbom_scanner`, `license_knowledge_base`, `release_pipeline`. Shape:
  `commit_time_gate`. Cluster: `license_reasoning_automation`. Ambiguous
  obligations routed to legal review.
- `SEC-006`: Insider Risk Legal Escalation Packet Builder. Surface:
  `ueba_alerts`, `case_management`, `legal_dms`. Shape:
  `packet_assembly_with_counsel_review`. Cluster:
  `cross_functional_case_normalization`. Standardized legal escalation inputs.
- `GOV-007`: Policy Exception Expiry Enforcer. Surface: `exception_registry`,
  `policy_engine`, `ticketing`. Shape: `nightly_expiry_job`. Cluster:
  `exception_lifecycle_governance`. Auto-revoke and renewal workflow for stale
  exceptions.
- `PRV-008`: Breach Notification Decision Engine. Surface: `dlp`, `siem`,
  `jurisdiction_rules`, `incident_platform`. Shape:
  `event_trigger_threshold_eval`. Cluster: `jurisdiction_decision_logic`.
  Statutory notice decisions with legal signoff.
- `GOV-009`: AI Model Release Governance Gate. Surface: `model_registry`,
  `risk_scoring`, `cicd`. Shape: `release_time_gate`. Cluster:
  `ai_governance_enforcement`. Blocks ungoverned model releases.
- `CMP-010`: Export Control Pre-Shipment Screening Flow. Surface: `erp_shipping`,
  `denied_party_lists`, `broker_integration`. Shape:
  `shipment_trigger_screen_hold`. Cluster: `export_screening_orchestration`.
  Counterparty screening at dispatch time.
- `LGL-011`: Retention-vs-Hold Conflict Detector. Surface:
  `records_management`, `legal_hold`, `backup_catalog`. Shape:
  `policy_update_conflict_scan`. Cluster: `retention_hold_conflict_logic`.
  Prevents unlawful deletion collisions.
- `PRV-012`: SCC Annex Drift Updater. Surface: `ropa`, `subprocessor_registry`,
  `contract_repository`. Shape: `weekly_diff_scan`. Cluster:
  `transfer_document_synchronization`. Transfer docs kept current with vendor
  changes.
- `SEC-013`: Privileged Session Policy Violation Escalator. Surface:
  `pam_session_logs`, `policy_engine`, `soar`. Shape:
  `real_time_stream_evaluation`. Cluster:
  `privileged_telemetry_normalization`. Real-time session-policy containment.
- `CMP-014`: Regulator Filing Clock Calculator. Surface:
  `incident_response_platform`, `statutory_rules`, `calendar_api`. Shape:
  `milestone_deadline_computation`. Cluster:
  `statutory_timeline_computation`. Filing clocks from incident milestones.
- `GOV-015`: Board Risk Packet Lineage Assembler. Surface: `risk_warehouse`,
  `kpi_apis`, `board_portal`. Shape: `month_end_batch`. Cluster:
  `executive_reporting_lineage`. Board packets with source-system traceability.
- `PRV-016`: Regional Consent Configuration Drift Monitor. Surface:
  `consent_platform`, `web_config_repo`, `synthetic_tests`. Shape:
  `daily_region_checks`. Cluster: `consent_configuration_governance`. Region
  behavior matched against approved legal configurations.

### Developer Productivity, CI, and Release

- `dp-001`: Targeted Reviewer Suggest and Evidence Pack. Surface: `git_diff`,
  `ownership_rules`, `static_analysis_summaries`. Shape: `one_shot_report`.
  Cluster: `diff_to_action`. Reviewable chunks with reviewer suggestions and
  risk hotspots.
- `dp-002`: PR Comment-to-Patch Queue Builder. Surface: `pr_review_comments`,
  `workspace_edits`. Shape: `interactive_queue`. Cluster:
  `feedback_tracking`. Ordered fix queues tied to files and lines.
- `dp-003`: What Changed API Surface Detector. Surface:
  `symbol_graph`, `git_diff`. Shape: `batch_analysis`. Cluster:
  `change_understanding`. Public API deltas for dependent updates and docs.
- `dp-004`: Minimal Repro Extractor for Failing Tests. Surface: `ci_logs`,
  `workspace_test_runner`. Shape: `interactive_narrowing_loop`. Cluster:
  `log_triage`. Smallest reproducible command or environment.
- `dp-005`: Flake Classifier and Quarantine Proposal. Surface: `ci_history`,
  `test_outcomes`, `timing`. Shape: `one_shot_report`. Cluster: `flakiness`.
  Flaky versus deterministic failures with supporting statistics.
- `dp-006`: Dependency Upgrade Risk Slice. Surface: `lockfile_diff`,
  `changelog_snippets`, `affected_import_graph`. Shape: `one_shot_report`.
  Cluster: `dependency_hygiene`. Only changed transitive edges and affected
  modules.
- `dp-007`: License and Attribution Delta Gate for New Artifacts. Surface:
  `build_outputs`, `package_manifests`, `license_scanners`. Shape:
  `one_shot_gate`. Cluster: `compliance`. Third-party attribution drift before
  release.
- `dp-008`: Changelog Entry Synthesizer from Merged PRs. Surface:
  `merged_pr_metadata`, `labels`, `diff_summaries`. Shape: `batch_generation`.
  Cluster: `release_notes`. Consistent human-editable release notes.
- `dp-009`: Schema Migration Safety Checklist Generator. Surface:
  `migration_files`, `orm_schema`, `query_usage_search`. Shape:
  `one_shot_report`. Cluster: `data_safety`. Backward-compat risks in database
  changes.
- `dp-010`: Build Cache Key Drift Inspector. Surface:
  `build_config`, `env_vars`, `cache_metadata`. Shape: `interactive_compare`.
  Cluster: `build_performance`. Why cache misses happen.
- `dp-011`: Rollback Candidate Locator. Surface: `artifact_registry_tags`,
  `commit_mapping`, `smoke_results`. Shape: `one_shot_shortlist`. Cluster:
  `rollback_ops`. Safest rollback targets from provenance and known-good
  signals.
- `dp-012`: Security Control Fail-Closed Coverage Finder. Surface:
  `code_search`, `config_parsing`, `policy_assertions`. Shape: `batch_report`.
  Cluster: `policy_enforcement`. Silent allow paths and targeted tests.
- `dp-013`: Monorepo Affected-Projects Test Matrix Pruner. Surface:
  `dependency_graph`, `git_diff`, `test_manifest`. Shape: `one_shot_plan`.
  Cluster: `compute_waste`. Minimal defensible impacted test set.
- `dp-014`: Code Ownership Drift Reconciler. Surface: `codeowners`,
  `team_directory`, `repo_paths`. Shape: `interactive_suggestions`. Cluster:
  `review_latency`. Uncovered paths and mismatched owners.
- `dp-015`: Release Artifact Provenance Attestor. Surface: `build_logs`,
  `git_sha`, `sbom`, `checksum_store`. Shape: `one_shot_attestation`. Cluster:
  `artifact_provenance`. Traceable artifacts tied to exact source and build
  inputs.
- `dp-016`: PR Risk Heatmap with Test and Doc Gaps. Surface: `diff`,
  `test_coverage_hints`, `doc_link_checks`. Shape: `one_shot_report`. Cluster:
  `change_risk`. Sensitive areas missing tests or docs.

### Supply Chain, Marketplace, Trust and Safety, and Logistics

- `SC-001`: Supplier Master Dedup. Surface: `search`, `compare`, `merge`.
  Shape: `batch`. Cluster: `entity_linkage`. Duplicate supplier records before
  onboarding.
- `MP-002`: Restricted Listing Triage. Surface: `fetch`, `classify`, `hold`.
  Shape: `real_time`. Cluster: `policy_routing`. Banned terms and risky listing
  combinations.
- `T&S-003`: Seller Identity Escalation. Surface: `read`, `verify`,
  `escalate`. Shape: `human_in_loop`. Cluster: `identity_linkage`. Identity
  docs, device signals, and registration disagreements.
- `LG-004`: ETA Drift Alerting. Surface: `track`, `predict`, `notify`. Shape:
  `streaming`. Cluster: `event_lag`. Shipment ETA slippage and alerting.
- `FR-005`: Account Creation Velocity Check. Surface: `ingest`, `score`,
  `block`. Shape: `real_time`. Cluster: `velocity_burst`. Signup bursts across
  IP, device, and phone reuse.
- `SC-006`: PO Substitution Approval. Surface: `diff`, `approve`, `publish`.
  Shape: `human_in_loop`. Cluster: `line_item_delta`. Supplier substitution
  changes for material, size, or origin.
- `MP-007`: Counterfeit Risk Sweep. Surface: `match`, `rank`, `case`. Shape:
  `batch`. Cluster: `catalog_mismatch`. Offer listings against brand-catalog
  signals.
- `LG-008`: Customs Doc Completeness. Surface: `parse`, `validate`, `hold`.
  Shape: `pre_shipment`. Cluster: `doc_gap`. Commercial invoices, HS codes, and
  consignee data.
- `T&S-009`: Review Spam Cluster. Surface: `cluster`, `score`, `remove`. Shape:
  `batch`. Cluster: `coordinated_activity`. Bursty review patterns by text and
  timing reuse.
- `FR-010`: Coupon Abuse Graph. Surface: `join`, `score`, `limit`. Shape:
  `near_real_time`. Cluster: `promo_abuse`. Linked accounts exploiting first
  order and referral promotions.
- `SC-011`: Carrier SLA Breach. Surface: `measure`, `escalate`, `reroute`.
  Shape: `streaming`. Cluster: `carrier_risk`. Repeated on-time failures before
  service degradation spreads.
- `MP-012`: High-Risk Return Screen. Surface: `inspect`, `flag`, `route`.
  Shape: `human_in_loop`. Cluster: `return_fraud`. Suspicious return requests
  with mismatched photos or serials.
- `LG-013`: Cross-Dock Exception. Surface: `reconcile`, `dispatch`, `alert`.
  Shape: `real_time`. Cluster: `transfer_mismatch`. Pallet, seal, or
  destination mismatches during transfers.
- `T&S-014`: Impersonation Takeover Check. Surface: `compare`, `challenge`,
  `lock`. Shape: `real_time`. Cluster: `account_takeover`. Email, device, and
  login geography mismatch.
- `FR-015`: BIN Pattern Drift. Surface: `monitor`, `score`, `decline`. Shape:
  `streaming`. Cluster: `card_signal_drift`. Card testing patterns across BIN
  and issuer changes.
- `SC-016`: Forecast Signal Refresh. Surface: `ingest`, `retrain`, `publish`.
  Shape: `scheduled`. Cluster: `demand_shift`. Replenishment signal refresh
  from sales and inventory data.

### Customer Support, Sales Ops, Success, and Account Management

- `CS-001`: Prevent Duplicate Goodwill Credits Before Payout. Surface:
  `ticketing_search`, `payments_api`, `credit_policy_engine`, `hold_action`.
  Shape: `real_time_pre_disbursement_check`. Cluster: `policy_rules`. Double
  credits across reopened cases.
- `CS-002`: Auto-Rebalance Breach-Risk Queue by Skill and SLA Clock. Surface:
  `queue_metrics`, `agent_skill_matrix`, `reassignment_endpoint`. Shape:
  `constrained_scheduler`. Cluster: `workflow_orchestration`. Imminent SLA
  misses routed to the right agents.
- `CS-003`: Build Engineering Escalation Packet with Repro Artifacts. Surface:
  `issue_tracker_create`, `logs_retrieval`, `session_replay_linker`,
  `redaction_service`. Shape: `one_shot_with_validation_gate`. Cluster:
  `data_normalization`. Privacy-safe escalation bundles for engineering.
- `CS-004`: Detect Policy-Exception Refund Patterns by Agent Cohort. Surface:
  `refund_ledger_query`, `iam_roster`, `anomaly_detection`,
  `audit_task_create`. Shape: `nightly_threshold_alerting`. Cluster:
  `governance_compliance`. Outlier exception behavior for coaching or controls.
- `SO-001`: Reconcile Territory Ownership After HR Moves. Surface:
  `hris_delta_feed`, `crm_owner_api`, `territory_rules_engine`. Shape:
  `event_driven_diff_writeback`. Cluster: `cross_system_identity`. Orphaned
  accounts after team changes.
- `SO-002`: Enforce Quote Margin Floors Pre-Approval. Surface:
  `cpq_line_item_api`, `cost_catalog`, `approval_workflow_api`. Shape:
  `synchronous_pre_submit_validation`. Cluster: `pricing_freshness`.
  Margin-eroding quotes blocked unless approved.
- `SO-003`: Surface Silent Pipeline Stalls and Auto-Create Next-Step Tasks.
  Surface: `crm_opportunity_history`, `activity_logs`, `task_creation_api`.
  Shape: `daily_scoring_and_action_injection`. Cluster:
  `activity_signal_quality`. Inactive late-stage deals turned into follow-up
  actions.
- `SO-004`: Resolve Partner Deal Registration Collisions. Surface:
  `prm_registrations`, `crm_opportunity_matcher`, `conflict_rules`,
  `notification_api`. Shape: `event_triggered_matching`. Cluster:
  `entity_resolution`. Overlapping partner claims routed for arbitration.
- `CXS-001`: Trigger Adoption-Cliff Interventions from Product Telemetry.
  Surface: `product_usage_warehouse`, `health_model`,
  `success_playbook_task_api`. Shape: `hourly_threshold_scoring`. Cluster:
  `model_calibration`. Sudden usage drops tied to playbook launches.
- `CXS-002`: Detect Executive Sponsor Changes in Customer Org Charts. Surface:
  `enrichment_provider`, `email_graph`, `crm_contact_roles_update`. Shape:
  `weekly_confidence_scan`. Cluster: `external_data_reliability`. Stakeholder
  maps kept current.
- `CXS-003`: Unblock Onboarding by Dependency Deadline Slippage. Surface:
  `onboarding_checklist_system`, `integration_status_api`, `csm_tasking`.
  Shape: `daily_critical_path_monitor`. Cluster: `multi_system_state`. Stalled
  prerequisites before launch slips.
- `CXS-004`: Backtest Health-Score Drivers Against Churn Outcomes. Surface:
  `churn_outcomes_dataset`, `feature_store`, `model_explainability_api`.
  Shape: `monthly_backtest`. Cluster: `analytics_backtest`. Weak predictors
  replaced with validated signals.
- `AM-001`: Correct Entitlement vs Contract Mismatches Before QBR. Surface:
  `contract_repository`, `provisioning_api`, `sku_mapping_table`,
  `case_creator`. Shape: `scheduled_reconciliation`. Cluster: `sku_taxonomy`.
  Under- or over-provisioned entitlements before renewals.
- `AM-002`: Identify White-Space Expansion via Feature-Use vs License Gap.
  Surface: `license_ledger`, `feature_adoption_metrics`, `org_hierarchy_map`.
  Shape: `weekly_account_scoring`. Cluster: `product_packaging_map`.
  Credible expansion paths tied to real unmet usage.
- `AM-003`: Assemble Invoice Dispute Root-Cause Evidence Pack. Surface:
  `billing_events_api`, `usage_audit_logs`, `contract_terms_parser`,
  `pdf_assembler`. Shape: `on_demand_trace_reconstruction`. Cluster:
  `audit_trail_fragmentation`. Verifiable timeline from usage to invoice.
- `AM-004`: Generate Co-Term Consolidation Scenarios Across Subsidiaries.
  Surface: `subscription_catalog`, `contract_dates`, `pricing_rules`,
  `scenario_calculator`. Shape: `batch_simulation`. Cluster:
  `contract_standardization`. Consolidation options across related entities.

### Healthcare, Public Sector, Research, Education, and Other Regulated Ops

- `hc_ops_001`: Prior Authorization Packet Assembly. Surface:
  `ehr_note_retrieval`, `document_lookup`, `payer_checklist_validation`,
  `draft_packet_creation`. Shape: `read_extract_compare_assemble`. Cluster:
  `documentation_completeness`. Internal prior-auth packet prep only.
- `hc_ops_002`: Referral Status Exception Triage. Surface:
  `referral_queue_search`, `status_parsing`, `worklist_routing`,
  `templated_outbound_messages`. Shape: `search_categorize_route`. Cluster:
  `queue_reconciliation`. Overdue or rejected referrals routed for follow-up.
- `hc_ops_003`: Credentialing Renewal Tracker. Surface:
  `license_lookup`, `expiration_calendar`, `reminder_drafting`,
  `file_attachment_indexing`. Shape: `ingest_check_remind`. Cluster:
  `credential_expiration_management`. Provider credentialing document gaps.
- `pubsec_001`: Public Records Intake Routing. Surface: `inbox_triage`,
  `form_metadata_extraction`, `department_directory_lookup`,
  `case_assignment`. Shape: `read_classify_assign`. Cluster:
  `intake_classification`. Request routing to the right statutory office.
- `pubsec_002`: Grant Compliance Deadline Monitor. Surface:
  `award_document_parsing`, `deadline_calendar`, `checklist_verification`,
  `notice_drafting`. Shape: `scan_extract_compare_alert`. Cluster:
  `compliance_deadline_tracking`. Reporting due dates and late notices.
- `pubsec_003`: Procurement Bid Responsiveness Check. Surface:
  `solicitation_search`, `clause_extraction`, `response_matrix_generation`,
  `missing_item_detection`. Shape: `read_compare_flag`. Cluster:
  `bid_completeness`. Mandatory RFP sections versus vendor responses.
- `research_001`: IRB Amendment Packet Prep. Surface:
  `protocol_document_retrieval`, `version_comparison`, `form_prefill`,
  `approval_routing`. Shape: `compare_prefill_route`. Cluster:
  `protocol_change_management`. Administrative amendment review packets.
- `research_002`: Study Enrollment Report Reconciliation. Surface:
  `subject_roster_lookup`, `source_report_comparison`,
  `discrepancy_summarization`, `dashboard_update`. Shape:
  `pull_compare_notify`. Cluster: `data_reconciliation`. CTMS versus source
  enrollment counts.
- `research_003`: Sponsored Spend Variance Review. Surface:
  `ledger_extraction`, `budget_line_mapping`, `variance_calculation`,
  `exception_note_drafting`. Shape: `fetch_map_compute_route`. Cluster:
  `budget_variance`. Sponsored-project spend exceptions.
- `edu_ops_001`: Term Schedule Conflict Resolver. Surface:
  `course_catalog_lookup`, `room_constraint_checking`,
  `draft_schedule_alternatives`. Shape: `detect_generate_prepare`. Cluster:
  `scheduling_conflict_resolution`. Registrar-style timetable conflicts.
- `edu_ops_002`: Accreditation Evidence Indexing. Surface:
  `policy_document_search`, `evidence_tagging`, `standard_mapping`,
  `archive_indexing`. Shape: `ingest_tag_detect`. Cluster:
  `evidence_completeness`. Missing artifacts against accreditation standards.
- `edu_ops_003`: Financial Aid Verification Queue. Surface:
  `application_field_checks`, `document_matching`, `exception_routing`,
  `student_notice_drafting`. Shape: `compare_identify_route`. Cluster:
  `eligibility_documentation`. Administrative completeness checks only.
- `reg_ops_001`: HIPAA Access Audit Sampling. Surface: `access_log_search`,
  `sampling_rules`, `exception_extraction`, `audit_packet_drafting`. Shape:
  `select_compare_flag`. Cluster: `access_audit`. Minimum-necessary anomalies
  from sampled access logs.
- `reg_ops_002`: Clinical Trial Vendor Onboarding Check. Surface:
  `vendor_questionnaire_parsing`, `policy_lookup`, `document_verification`,
  `approval_task_creation`. Shape: `review_verify_route`. Cluster:
  `vendor_due_diligence`. Required onboarding documents for regulated vendors.
- `reg_ops_003`: Utility Incident Notice Coordinator. Surface:
  `incident_log_intake`, `jurisdiction_lookup`, `notice_template_selection`,
  `delivery_tracking`. Shape: `read_determine_populate_schedule`. Cluster:
  `regulatory_notice_routing`. Timely notice templates by jurisdiction.
- `reg_ops_004`: Pharmacy Inventory Recall Triage. Surface: `lot_lookup`,
  `recall_bulletin_parsing`, `stock_location_search`,
  `quarantine_task_creation`. Shape: `ingest_match_generate`. Cluster:
  `recall_containment`. Lot matching and quarantine tasking for recalls.

## How To Use This File

If future work promotes any of these examples into the executable gallery, add
the actual guest program and catalog entry under
`examples/programmatic-tool-calls/`, run the audit, and then either:

- record the example as passing because the runtime and host contract support it
- or move the real failing behavior into `USE_CASE_GAPS.md` with the exact
  runtime gap, correctness bug, or deliberate fail-closed boundary

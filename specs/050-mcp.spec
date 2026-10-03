# MCP server — prefix MCP.
# Every scenario delegates to the mcp/cli Rust test that pins it (F29).

decision MCP-001 "The MCP server speaks NDJSON JSON-RPC over stdio with six typed tools" {
  status: accepted
  context: "F16 (README 'Using with AI agents'): `decispec mcp` serves newline-delimited JSON-RPC 2.0 so agents call typed tools instead of parsing CLI output — built from a real agent port where the friction was schemas-in-tools/list and JSON-inside-the-envelope results (F18 friction log)."
  consequences: "+ agents get structured answers with input schemas discoverable in-band. - the wire format is hand-rolled NDJSON; it is tested over buffers, not against the MCP reference client."
}

spec MCP-001 {
  requirement MCP-001-R1 (EARS) {
    text: "WHEN an agent connects over stdio THEN the server SHALL dispatch tools/list and tools/call envelopes, advertise exactly six tools with input schemas, and answer initialize without unsolicited notifications"
    layers: [unit]
    scenarios: [tools_list_has_six_tools_with_schemas, serve_io_dispatches_ndjson_over_buffers, initialize_result_and_notifications_silent, mcp_subcommand_parses_and_dispatches]
  }
  scenario tools_list_has_six_tools_with_schemas {
    given: "a running server"
    when: "tools/list is requested"
    then: "exactly six tools are advertised, each with its input schema"
    # delegates: decispec-mcp tests::tools_list_has_six_tools_with_schemas
  }
  scenario serve_io_dispatches_ndjson_over_buffers {
    given: "NDJSON requests on stdin"
    when: "serve_io dispatches them"
    then: "each request gets a well-formed JSON-RPC reply on stdout"
    # delegates: decispec-mcp tests::serve_io_dispatches_ndjson_over_buffers
  }
  scenario initialize_result_and_notifications_silent {
    given: "an initialize handshake"
    when: "the server replies"
    then: "the result is well-formed and no notifications are emitted"
    # delegates: decispec-mcp tests::initialize_result_and_notifications_silent
  }
  scenario mcp_subcommand_parses_and_dispatches {
    given: "the decispec mcp invocation"
    when: "the cli parses the subcommand"
    then: "it dispatches to the server entry point"
    # delegates: decispec (cli) tests::mcp_subcommand_parses_and_dispatches
  }
}

decision MCP-002 "The MCP tools answer the porting workflow without parsing CLI output" {
  status: accepted
  context: "README tool table: project_info, extract_project, validate_spec (iterate-before-you-write, diagnostics phase-by-phase), check_workspace, workspace_overview (the porting work queue), gate_status. F18's real-agent friction log shaped these: schemas live in tools/list, results are JSON inside the content envelope."
  consequences: "+ an agent can bootstrap, validate, and monitor a port entirely through typed calls. - tool payloads are a second surface to keep aligned with CLI behavior; nothing generates one from the other."
}

spec MCP-002 {
  requirement MCP-002-R1 (EARS) {
    text: "WHEN an agent calls the workspace tools THEN it SHALL receive the project overview, spec diagnostics, gate status, and an extract bootstrap as structured JSON"
    layers: [unit]
    scenarios: [validate_spec_reports_diagnostics_and_error_codes, workspace_overview_lists_porting_gaps, gate_status_reports_missing_and_parses_report, extract_project_returns_decisions_and_containers]
  }
  scenario validate_spec_reports_diagnostics_and_error_codes {
    given: "spec text with errors"
    when: "validate_spec is called"
    then: "file:line-style diagnostics and error codes come back as structured JSON"
    # delegates: decispec-mcp tests::validate_spec_reports_diagnostics_and_error_codes
  }
  scenario workspace_overview_lists_porting_gaps {
    given: "a workspace with coverage gaps"
    when: "workspace_overview is called"
    then: "the porting queue lists decisions missing requirements and requirements missing scenarios"
    # delegates: decispec-mcp tests::workspace_overview_lists_porting_gaps
  }
  scenario gate_status_reports_missing_and_parses_report {
    given: "a project with and then without a gate report"
    when: "gate_status is called"
    then: "it reports 'no report yet' first and parses the verdict after a run"
    # delegates: decispec-mcp tests::gate_status_reports_missing_and_parses_report
  }
  scenario extract_project_returns_decisions_and_containers {
    given: "an analyzable project"
    when: "extract_project is called"
    then: "the draft's decisions and containers come back as structured JSON"
    # delegates: decispec-mcp tests::extract_project_returns_decisions_and_containers
  }
}

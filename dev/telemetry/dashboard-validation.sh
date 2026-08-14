#!/usr/bin/env bash
# Shared Grafana dashboard query and semantic assertions.
# Callers must define grafana_endpoint before sourcing this file.

readonly dashboard_variable_names=(
  version_full
  release_channel
  agent_engine
  slash_command
  session_interface
  agent_mode
  os_type
  install_method
  model
  tool_origin
  execution_context
  mcp_server_source
  process_role
  auth_method
  instance
)

dashboard_interpolate_query() {
  local query="$1"
  local filter_mode="${2:-all}"
  local filter_value="${3:-}"
  local variable_name replacement

  for variable_name in "${dashboard_variable_names[@]}"; do
    replacement=".*"
    if [ "${filter_mode}" = "v1" ] && [ "${variable_name}" = "version_full" ]; then
      replacement="${filter_value}"
    elif [ "${filter_mode}" = "v1" ] && [ "${variable_name}" = "agent_engine" ]; then
      replacement="v1"
    elif [ "${filter_mode}" = "slash" ] && [ "${variable_name}" = "slash_command" ]; then
      replacement="${filter_value}"
    fi
    query="${query//\$${variable_name}/${replacement}}"
  done

  printf '%s' "${query}"
}

grafana_query() {
  local ref_id="$1"
  local datasource_uid="$2"
  local query="$3"
  local instant="$4"
  local range="$5"
  local from="$6"
  local filter_mode="${7:-all}"
  local filter_value="${8:-}"
  local payload

  query="$(dashboard_interpolate_query "${query}" "${filter_mode}" "${filter_value}")"
  payload="$(jq -cn \
    --arg ref_id "${ref_id}" \
    --arg datasource_uid "${datasource_uid}" \
    --arg query "${query}" \
    --arg from "${from}" \
    --argjson instant "${instant}" \
    --argjson range "${range}" \
    '{
      from: $from,
      to: "now",
      queries: [{
        refId: $ref_id,
        datasource: {uid: $datasource_uid, type: "prometheus"},
        expr: $query,
        format: "time_series",
        instant: $instant,
        range: $range,
        intervalMs: 5000,
        maxDataPoints: 1000
      }]
    }')"
  curl -sS -H 'Content-Type: application/json' --data "${payload}" \
    "${grafana_endpoint}/api/ds/query"
}

assert_dashboard_queries_populated() {
  local name="$1"
  local dashboard_response="$2"
  local panel_title ref_id datasource_uid query instant range from response

  while IFS=$'\t' read -r panel_title ref_id datasource_uid query instant range from; do
    response="$(grafana_query \
      "${ref_id}" "${datasource_uid}" "${query}" "${instant}" "${range}" "${from}")"
    if ! printf '%s' "${response}" | jq -e --arg ref_id "${ref_id}" '
      .results[$ref_id] as $result
      | ($result | type == "object")
        and ($result.error? == null)
        and (($result.frames // []) | length > 0)
        and ([($result.frames // [])[].data.values[1][]? | select(type == "number")] | length > 0)
    ' >/dev/null; then
      echo "Unpopulated Grafana query: ${name} / ${panel_title}" >&2
      printf '%s\n' "${response}" >&2
      exit 2
    fi
  done < <(printf '%s' "${dashboard_response}" | jq -r '
    .dashboard.panels[] as $panel
    | $panel.targets[]?
    | [$panel.title, .refId, .datasource.uid, .expr, (.instant | tostring), (.range | tostring), ($panel.timeFrom // "now-5m")]
    | @tsv
  ')
  echo "OBSERVED  all ${name} panel queries populated"
}

assert_grafana_panel_query() {
  local dashboard_response="$1"
  local title="$2"
  local expected="${3:-}"
  local filter_mode="${4:-all}"
  local filter_value="${5:-}"
  local targets ref_id datasource_uid query instant range from response

  targets="$(printf '%s' "${dashboard_response}" | jq -cer --arg title "${title}" \
    '[.dashboard.panels[] | select(.title == $title) as $panel | $panel.targets[]
      | [.refId, .datasource.uid, .expr, (.instant | tostring), (.range | tostring), ($panel.timeFrom // "now-5m")]]
     | select(length > 0)')"
  while IFS=$'\t' read -r ref_id datasource_uid query instant range from; do
    response="$(grafana_query \
      "${ref_id}" "${datasource_uid}" "${query}" "${instant}" "${range}" "${from}" \
      "${filter_mode}" "${filter_value}")"
    if ! printf '%s' "${response}" | jq -e --arg ref_id "${ref_id}" --arg expected "${expected}" '
      .results[$ref_id] as $result
      | ($result.error? == null)
        and (($result.frames // []) | length > 0)
        and ([($result.frames // [])[].data.values[1][]? | select(. != null)] as $values
          | ($values | length > 0)
            and all($values[];
              (type == "number")
              and ($expected == "" or . == ($expected | tonumber))))
    ' >/dev/null; then
      echo "Grafana query returned no expected data: ${title}" >&2
      printf '%s\n' "${response}" >&2
      exit 2
    fi
    echo "OBSERVED  Grafana panel: ${title}"
  done < <(printf '%s' "${targets}" | jq -r '.[] | @tsv')
}

assert_dashboard_semantics() {
  local detailed_dashboard_response="$1"
  local health_dashboard_response="$2"

  printf '%s' "${detailed_dashboard_response}" | jq -e '
    [.dashboard.panels[] | select(.title == "Runs started by engine") | .targets[]] as $counterSeries
    | [.dashboard.panels[] | select(.title == "Chat session share by agent mode") | .targets[]] as $counterTotals
    | [.dashboard.panels[] | select(.title == "Startup duration mean by engine") | .targets[]] as $histograms
    | [.dashboard.panels[] | select(.title == "Slash commands invoked") | .targets[].expr] as $slash
    | [.dashboard.panels[] | select(.title == "Top-level commands invoked") | .targets[].expr] as $topLevel
    | ([.dashboard.panels[].targets[]? | select(.range and (.expr | contains("[$__range]")))] | length == 0)
      and ([.dashboard.panels[].targets[]? | select(.instant and (.expr | contains("[$__rate_interval]")))] | length == 0)
      and ($counterSeries | length > 0 and all(.[];
        .range and (.instant | not) and (.expr | contains("increase(")) and (.expr | contains("[$__rate_interval]"))))
      and ($counterTotals | length > 0 and all(.[];
        .instant and (.range | not) and (.expr | contains("increase(")) and (.expr | contains("[$__range]"))))
      and ($histograms | length > 0 and all(.[]; .expr | contains("rate(") and contains("[$__rate_interval]")))
      and ($slash | length > 0 and all(.[]; contains("slash_command=~\"$slash_command\"")))
      and ($topLevel | length > 0 and all(.[]; contains("$slash_command") | not))
  ' >/dev/null

  printf '%s' "${health_dashboard_response}" | jq -e '
    [.dashboard.panels[]
      | select(.title == "Daily active installations for selected version"
        or .title == "Selected version share of daily active installations (%)"
        or .title == "Active installation share by OS"
        or .title == "Active installation share by install method")] as $daily
    | [.dashboard.panels[]
      | select(.title == "Client-turn availability by engine (%)")
      | .targets[].expr] as $availability
    | (.dashboard.timezone == "utc")
      and ($daily | length == 4 and all(.[];
        .timeFrom == "now/d"
        and all(.targets[];
          .instant and (.range | not)
          and (.expr | contains("increase("))
          and (.expr | contains("[$__range]"))
          and ((.expr | contains("[1d]")) | not))))
      and ($availability | length > 0 and all(.[];
        contains("> 0") and (contains("clamp_min") | not)))
      and ([.dashboard.panels[].targets[]? | select(.range and (.expr | contains("[$__range]")))] | length == 0)
      and ([.dashboard.panels[].targets[]? | select(.instant and (.expr | contains("[$__rate_interval]")))] | length == 0)
  ' >/dev/null

  echo 'OBSERVED  target-aware counter windows, UTC-day activity, no-data guards, and slash-command isolation'
}

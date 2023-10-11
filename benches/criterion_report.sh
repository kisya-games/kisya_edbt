#!/usr/bin/env bash
# Summarise the `update_duration_per_actor` benchmark group into a per-actor Markdown table.
#
# Reads $CRITERION_TARGET/update_duration_per_actor/<scenario>/<N>/new/estimates.json and prints,
# for each scenario and entity count N, the per-actor cost (total update time
# divided by N).
#
# Usage: benches/criterion_report.sh   # run the benchmark first
# Override the results location with CRITERION_TARGET (default: target/criterion).
#
# Dependencies: jq.

CRITERION_TARGET="${CRITERION_TARGET:-target/criterion}"

group_dir="${CRITERION_TARGET}/update_duration_per_actor"
if [[ ! -d "$group_dir" ]]; then
  echo "no results at $group_dir (run the benchmark first)" >&2
  exit 1
fi

command -v jq >/dev/null || { echo "jq not found" >&2; exit 1; }

rows=""
for est in "$group_dir"/*/*/new/estimates.json; do
  [[ -e "$est" ]] || continue
  point_dir="${est%/new/estimates.json}"
  n="$(basename "$point_dir")"
  scenario="$(basename "$(dirname "$point_dir")")"
  [[ "$n" =~ ^[0-9]+$ ]] || continue
  read -r total_ms update_duration_per_actor_us actors_per_sec < <(
    jq -r --argjson n "$n" \
      '.mean.point_estimate | "\(. / 1e6) \((. / $n) / 1e3) \($n * 1e9 / .)"' "$est"
  )
  rows+="$scenario $n $total_ms $update_duration_per_actor_us"$'\n'
done

[[ -n "$rows" ]] || { echo "no <scenario>/<N> estimates under $group_dir" >&2; exit 1; }

# Sort by scenario, then numerically by N.
rows="$(printf '%s' "$rows" | sort -k1,1 -k2,2n)"

printf '| scenario | actors | total | per actor |\n'
printf '|---|---:|---:|---:|\n'
while read -r scenario n total_ms update_duration_per_actor_us; do
  printf '| _%s_ | %d | %.3f ms | %.3f µs |\n' \
    "$scenario" "$n" "$total_ms" "$update_duration_per_actor_us"
done <<< "$rows"

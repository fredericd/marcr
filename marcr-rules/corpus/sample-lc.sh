#!/bin/bash
# Picks random Library of Congress records from its SRU server and prints
# their control numbers (001), one per line, leaving out those already in
# the given files. Each query takes a few records at a random position in
# its results; the server limits its rate, hence the pause. Combine
# several queries for variety, as only the first 10,000 results of a query
# can be reached.
#
# Usage: corpus/sample-lc.sh COUNT QUERY... [-x FILE]... >> corpus/lc.txt
# QUERY is CQL: 'dc.title=history', 'dc.subject=maps', ...; COUNT records
# are picked for each query.
set -euo pipefail

sru='http://lx2.loc.gov:210/LCDB?operation=searchRetrieve&version=1.1&recordSchema=marcxml'
per_query=5
pause=4

count=$1
shift
queries=()
exclude=$(mktemp)
trap 'rm -f "$exclude"' EXIT
while [ $# -gt 0 ]; do
    case $1 in
        -x) cat "$2" >> "$exclude"; shift 2 ;;
        *) queries+=("$1"); shift ;;
    esac
done

for query in "${queries[@]}"; do
    q=${query// /%20}
    found=0
    tries=0
    total=$(curl -s --max-time 60 "$sru&query=$q&maximumRecords=1" \
        | sed -n 's/.*<zs:numberOfRecords>\([0-9]*\)<.*/\1/p')
    sleep $pause
    [ -n "$total" ] && [ "$total" -gt 0 ] || { echo "$query: no result" >&2; continue; }
    # The server serves the first 10,000 results only
    [ "$total" -le 10000 ] || total=10000
    while [ $found -lt "$count" ] && [ $tries -lt $((count * 3)) ]; do
        tries=$((tries + 1))
        # 30-bit random start, so that large result sets are covered
        start=$(((RANDOM * 32768 + RANDOM) % total + 1))
        for id in $(curl -s --max-time 60 "$sru&query=$q&maximumRecords=$per_query&startRecord=$start" \
            | sed -n 's/.*tag="001">\([^<]*\)<.*/\1/p' | tr -d ' '); do
            grep -qx "$id" "$exclude" && continue
            echo "$id" >> "$exclude"
            echo "$id"
            found=$((found + 1))
            [ $found -lt "$count" ] || break
        done
        sleep $pause
    done
    echo "$query: $found" >&2
done

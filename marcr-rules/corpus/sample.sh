#!/bin/bash
# Picks random Sudoc PPNs by type of document, from the Sudoc SRU
# server, and prints them one per line, leaving out those already in the
# given PPN files. Each query combines a type with a common title word,
# and takes a few records at a random position in its results.
#
# Usage: corpus/sample.sh COUNT TYPE... [-x PPN_FILE]... >> corpus/ppn.txt
# Types (Sudoc index tdo): b books, t serials, k maps, m scores,
# n sound recordings, v videos, i images, f manuscripts, y theses,
# o electronic resources.
set -euo pipefail

sru='https://www.sudoc.abes.fr/cbs/sru/?operation=searchRetrieve&version=1.1&recordSchema=unimarc'
words=(de la le et des du en the of and in a der die und el los di il)
per_query=5

count=$1
shift
types=()
exclude=$(mktemp)
trap 'rm -f "$exclude"' EXIT
while [ $# -gt 0 ]; do
    case $1 in
        -x) cat "$2" >> "$exclude"; shift 2 ;;
        *) types+=("$1"); shift ;;
    esac
done

for type in "${types[@]}"; do
    found=0
    tries=0
    while [ $found -lt "$count" ] && [ $tries -lt $((count * 3)) ]; do
        tries=$((tries + 1))
        word=${words[RANDOM % ${#words[@]}]}
        query="tdo%3D$type%20and%20mti%3D$word"
        total=$(curl -s --max-time 60 "$sru&query=$query&maximumRecords=1" \
            | sed -n 's/.*<srw:numberOfRecords>\([0-9]*\)<.*/\1/p')
        [ -n "$total" ] && [ "$total" -gt 0 ] || continue
        # 30-bit random start, so that large result sets are covered
        start=$(((RANDOM * 32768 + RANDOM) % total + 1))
        for ppn in $(curl -s --max-time 60 "$sru&query=$query&maximumRecords=$per_query&startRecord=$start" \
            | sed -n 's/.*tag="001">\([^<]*\)<.*/\1/p'); do
            grep -qx "$ppn" "$exclude" && continue
            echo "$ppn" >> "$exclude"
            echo "$ppn"
            found=$((found + 1))
            [ $found -lt "$count" ] || break
        done
        sleep 0.5
    done
    echo "$type: $found" >&2
done

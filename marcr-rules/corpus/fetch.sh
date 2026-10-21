#!/bin/bash
# Fetches the Sudoc records listed in ppn.txt (one PPN per line) from the
# Abes public Z39.50 server, in UNIMARC (unimarc.mrc) and in MARC21
# (marc21.mrc), UTF-8 encoded. Records are paired by PPN (field 001), not
# by position. Requires yaz-client (yaz package).
#
# Usage: corpus/fetch.sh [PPN_FILE]
set -euo pipefail

cd "$(dirname "$0")"
ppn_file=${1:-ppn.txt}
server=carmin.sudoc.abes.fr:210/abes-z39-public
batch=50 # PPNs per query, OR-ed together

rm -f unimarc.mrc marc21.mrc
ppns=()
while read -r ppn; do [ -n "$ppn" ] && ppns+=("$ppn"); done < "$ppn_file" # no mapfile in macOS bash 3.2
for ((i = 0; i < ${#ppns[@]}; i += batch)); do
    chunk=("${ppns[@]:i:batch}")
    # RPN: @or @or a b c... with one "@or" less than terms
    query=""
    for ((k = 1; k < ${#chunk[@]}; k++)); do query+="@or "; done
    for ppn in "${chunk[@]}"; do query+="@attr 1=12 $ppn "; done
    for syntax in unimarc usmarc; do
        out=unimarc.mrc; [ $syntax = usmarc ] && out=marc21.mrc
        printf 'charset UTF-8\nopen %s\nformat %s\nfind %s\nshow 1+%d\nquit\n' \
            "$server" "$syntax" "$query" "${#chunk[@]}" \
            | yaz-client -m "$out" > /dev/null 2>&1
    done
    echo "$((i + ${#chunk[@]}))/${#ppns[@]}" >&2
done

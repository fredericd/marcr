#!/bin/bash
# Fetches the Library of Congress records listed in a file (one control
# number, 001, per line) from its SRU server, and writes them as MARC21
# ISO 2709 to lc.mrc, or lc-NAME.mrc. Requires marcr (cargo install marcr,
# or MARCR=path/to/marcr).
#
# Usage: corpus/fetch-lc.sh [ID_FILE [NAME]]
set -euo pipefail
marcr=${MARCR:-marcr}

cd "$(dirname "$0")"
id_file=${1:-lc.txt}
out=lc${2:+-$2}.mrc
sru='http://lx2.loc.gov:210/LCDB?operation=searchRetrieve&version=1.1&recordSchema=marcxml'
batch=20 # ids per query, OR-ed together
pause=4  # the server limits its rate

xml=$(mktemp)
trap 'rm -f "$xml"' EXIT
echo '<collection xmlns="http://www.loc.gov/MARC21/slim">' > "$xml"
ids=()
while read -r id; do [ -n "$id" ] && ids+=("$id"); done < "$id_file"
for ((i = 0; i < ${#ids[@]}; i += batch)); do
    chunk=("${ids[@]:i:batch}")
    query="rec.id=${chunk[0]}"
    for id in "${chunk[@]:1}"; do query+="%20or%20rec.id=$id"; done
    # Keep the <record> elements only
    curl -s --max-time 120 "$sru&query=$query&maximumRecords=$batch" \
        | perl -0777 -ne 'print "<record>$1</record>\n" while /<record xmlns="[^"]*">(.*?)<\/record>/sg' >> "$xml"
    echo "$((i + ${#chunk[@]}))/${#ids[@]}" >&2
    sleep $pause
done
echo '</collection>' >> "$xml"
"$marcr" -d marcxml -s iso2709 -o "$out" "$xml"

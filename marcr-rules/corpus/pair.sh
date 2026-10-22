#!/bin/bash
# Looks up in the Sudoc, by ISBN or else ISSN, the records of a MARC21 file
# (ISO 2709), and prints the pairs found, "001 PPN", one per line. The
# first Sudoc record found is taken. Requires marcr (cargo install marcr,
# or MARCR=path/to/marcr).
#
# Usage: corpus/pair.sh lc.mrc > lc-pairs.txt
set -euo pipefail
marcr=${MARCR:-marcr}

sru='https://www.sudoc.abes.fr/cbs/sru/?operation=searchRetrieve&version=1.1&recordSchema=unimarc&maximumRecords=1'

# 001, first ISBN, first ISSN (shortcut: other 020/022 are not tried).
# Tabs become US: read merges consecutive tabs, losing empty columns.
"$marcr" --fields 001,020a,022a "$1" | tail -n +2 | tr '\t' '\037' | while IFS=$'\037' read -r id isbn issn; do
    isbn=$(echo "$isbn" | sed 's/[^0-9Xx].*//')
    issn=$(echo "$issn" | sed 's/[^0-9Xx-].*//')
    if [ -n "$isbn" ]; then
        query="isb%3D$isbn"
    elif [ -n "$issn" ]; then
        query="isn%3D$issn"
    else
        continue
    fi
    ppn=$(curl -s --max-time 60 "$sru&query=$query" | sed -n 's/.*tag="001">\([^<]*\)<.*/\1/p' | head -1)
    [ -n "$ppn" ] && echo "$id $ppn"
    sleep 0.5
done

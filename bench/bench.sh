#!/usr/bin/env bash
# Benchmarks the marcr command-line tool against yaz-marcdump.
#
# Usage: bench/bench.sh FILE.mrc [RUNS]
#
# FILE.mrc is a UTF-8 ISO 2709 file. A MARCXML copy (FILE.xml) is
# generated next to it on the first run, for the MARCXML input case.
# Every conversion is run once to warm the file cache, then RUNS times
# (default 3), its output discarded. Prints the median wall-clock time
# and the peak memory of each tool.
#
# marcr processes records on all cores by default, yaz-marcdump on a
# single one: marcr is measured both with -j 1 (single thread) and on all
# cores. The speedup is yaz-marcdump's time divided by marcr's.
set -euo pipefail

input=${1:?Usage: bench/bench.sh FILE.mrc [RUNS]}
runs=${2:-3}
root=$(cd "$(dirname "$0")/.." && pwd)
marcr="$root/target/release/marcr"
xml="${input%.*}.xml"

command -v yaz-marcdump >/dev/null || { echo "yaz-marcdump not found" >&2; exit 1; }
(cd "$root" && cargo build --release --quiet)

if [[ ! -f $xml ]]; then
    echo "Generating $xml" >&2
    # Exit status 2 only means some records were skipped
    "$marcr" -d iso2709 -s marcxml -o "$xml" "$input" || [[ $? -eq 2 ]]
fi

# Runs a shell command, output discarded; prints "seconds peak_rss_bytes".
measure() {
    local out
    if [[ $(uname) == Darwin ]]; then
        out=$( { /usr/bin/time -l bash -c "$1 >/dev/null 2>&1"; } 2>&1 ) || true
        awk '/ real /{r=$1} /maximum resident set size/{m=$1} END{print r, m}' <<<"$out"
    else
        out=$( { /usr/bin/time -f '%e %M' bash -c "$1 >/dev/null 2>&1"; } 2>&1 ) || true
        tail -n 1 <<<"$out" | awk '{print $1, $2 * 1024}'
    fi
}

median() {
    printf '%s\n' "$@" | sort -n | awk '{a[NR] = $1}
        END {print (NR % 2) ? a[(NR + 1) / 2] : (a[NR / 2] + a[NR / 2 + 1]) / 2}'
}

# Runs `cmd` RUNS times; prints "median_seconds peak_rss_mb".
bench() {
    local cmd=$1 times=() peak=0 t m
    bash -c "$cmd >/dev/null 2>&1" || true # warm-up
    for _ in $(seq "$runs"); do
        read -r t m < <(measure "$cmd")
        times+=("$t")
        (( m > peak )) && peak=$m
    done
    echo "$(median "${times[@]}") $(awk -v m="$peak" 'BEGIN {printf "%.1f", m / 1048576}')"
}

# yaz-marcdump time / marcr time, or "-" when a run is too short to be timed.
speedup() {
    awk -v y="$1" -v m="$2" 'BEGIN {if (m > 0) printf "x%.1f", y / m; else print "-"}'
}

in_iso=$(printf '%q' "$input")
in_xml=$(printf '%q' "$xml")
cases=(
    "ISO 2709 -> ISO 2709|$marcr -d iso2709 -s iso2709 $in_iso|yaz-marcdump -i marc -o marc $in_iso"
    "ISO 2709 -> MARCXML|$marcr -d iso2709 -s marcxml $in_iso|yaz-marcdump -i marc -o marcxml $in_iso"
    "ISO 2709 -> text|$marcr -d iso2709 -s text $in_iso|yaz-marcdump -i marc -o line $in_iso"
    "MARCXML -> ISO 2709|$marcr -d marcxml -s iso2709 $in_xml|yaz-marcdump -i marcxml -o marc $in_xml"
)

echo "Input: $input ($runs runs, median, $(getconf _NPROCESSORS_ONLN) cores)"
echo "Speedup: yaz-marcdump time / marcr time"
echo
printf '%-22s | %-19s | %-27s | %-27s\n' "" "yaz-marcdump" "marcr -j 1" "marcr (all cores)"
printf '%-22s | %8s %10s | %8s %7s %10s | %8s %7s %10s\n' \
    "Conversion" "time" "memory" "time" "speedup" "memory" "time" "speedup" "memory"
for c in "${cases[@]}"; do
    IFS='|' read -r name marcr_cmd yaz_cmd <<<"$c"
    read -r yt ym < <(bench "$yaz_cmd")
    read -r st sm < <(bench "$marcr -j 1 ${marcr_cmd#"$marcr "}")
    read -r mt mm < <(bench "$marcr_cmd")
    printf '%-22s | %7ss %7s MB | %7ss %7s %7s MB | %7ss %7s %7s MB\n' \
        "$name" "$yt" "$ym" "$st" "$(speedup "$yt" "$st")" "$sm" "$mt" "$(speedup "$yt" "$mt")" "$mm"
done

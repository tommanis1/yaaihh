#!/usr/bin/env bash
# run.sh PATH-TO-eflint-rs: runs every example here and every scenario beside (../../scenarios) and counts its queries. An example is the eFLINT half of one of
# the programs, written by hand in the reference syntax eflint-rs reads, with a scenario; every query in it must
# succeed. Exit 1 if one fails or one file gives fewer answers than it has queries.
set -u
here=$(cd "$(dirname "$0")" && pwd)
bad=0
for f in "$here"/*.eflint "$here"/../../scenarios/*.eflint; do
  out=$("$1" "$f" </dev/null 2>&1)
  ok=$(printf '%s\n' "$out" | grep -c 'query successful')
  failed=$(printf '%s\n' "$out" | grep -c 'query failed')
  asked=$(grep -o '?[!A-Za-z]' "$f" | wc -l)
  printf '%-18s %2d of %2d queries succeed, %d failed\n' "$(basename "$f")" "$ok" "$asked" "$failed"
  [ "$failed" = 0 ] && [ "$ok" = "$asked" ] || bad=1
done
exit $bad

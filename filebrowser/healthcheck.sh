#!/bin/sh
set -eu
umask 077

fail() {
  printf 'FileBrowser health: %s\n' "$*" >&2
  exit 1
}

fail_storage() {
  printf 'FileBrowser health: %s: %s\n' "$2" "$1" >&2
  id >&2
  ls -ldn "${1%/*}" "$1" >&2 || :
  exit 1
}

probe_dir=
cleanup() {
  if [ -n "$probe_dir" ]; then
    rm -f "$probe_dir/probe"
    rmdir "$probe_dir"
  fi
}
trap cleanup EXIT
trap 'exit 1' HUP INT TERM

check_readable() {
  [ -e "$1" ] || [ -L "$1" ] || fail_storage "$1" 'path is missing or inaccessible'
  [ -d "$1" ] || fail_storage "$1" 'path is not a directory'
  [ -r "$1" ] || fail_storage "$1" 'directory is not readable'
  [ -x "$1" ] || fail_storage "$1" 'directory is not searchable'
  ls -A "$1" >/dev/null || fail_storage "$1" 'cannot list directory'
}

check_writable() {
  [ -w "$1" ] || fail_storage "$1" 'directory is not writable'
  probe_dir=$(mktemp -d "$1/.wonton-health.XXXXXX") ||
    fail_storage "$1" 'cannot create probe directory'
  printf 'wonton-health\n' >"$probe_dir/probe" || fail_storage "$1" 'cannot write probe'
  IFS= read -r result <"$probe_dir/probe" || fail_storage "$1" 'cannot read probe'
  [ "$result" = wonton-health ] || fail_storage "$1" 'probe content mismatch'
  rm "$probe_dir/probe" && rmdir "$probe_dir" || fail_storage "$1" 'cannot remove probe'
  probe_dir=
}

# /health only reports HTTP liveness in v2.0.2-stable. Check storage as the same UID
# as the app; probe roots only, not an expensive recursive scan of every file.
for dir in /data/documents /data/photos /data/music /data/videos /data/downloads /data/transfers \
  /home/filebrowser/data /home/filebrowser/data/cache; do
  check_readable "$dir"
done

# Only touch uniquely named private probes, never existing user files or read-only shares.
for dir in /data/documents /data/downloads /data/transfers \
  /home/filebrowser/data /home/filebrowser/data/cache; do
  check_writable "$dir"
done

curl --fail --silent --show-error --max-time 3 --output /dev/null \
  http://127.0.0.1:80/health || fail 'HTTP liveness request failed'

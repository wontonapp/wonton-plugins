#!/bin/sh
set -eu

[ -s /opt/couchdb/data/.wonton-initialized ] || {
  echo "CouchDB 初始化尚未完成" >&2
  exit 1
}

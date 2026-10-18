#!/usr/bin/env bash
# Smoke test of the server image (docs/plan/20-self-host-server.md §7): runs
# the image as built (read-only root, tmpfs /tmp, one data volume, a
# certificate from a throwaway CA), then health, register, sync and a
# second device's recovery with Lockdown's real CLI over hybrid TLS, a
# backup made and verified inside the container, and the memory it uses
# idle and after load.
#
#   bins/server/container/smoke.sh IMAGE LOCKDOWN_CLI
#
# Needs docker, openssl and curl. Leaves nothing running.
set -euo pipefail

IMAGE=${1:?image}
CLI=$(realpath "${2:?path to the lockdown CLI}")
PORT=${SMOKE_PORT:-18443}
NAME=lockdown-smoke-$$
WORK=$(mktemp -d)
VOLUME=$NAME-data
cleanup() {
  docker rm -f "$NAME" >/dev/null 2>&1 || true
  docker volume rm "$VOLUME" >/dev/null 2>&1 || true
  rm -rf "$WORK"
}
trap cleanup EXIT

say() { printf '\n== %s\n' "$*"; }

say "a throwaway CA and a certificate for localhost"
mkdir -p "$WORK/tls"
openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes -days 2 \
  -subj "/CN=Lockdown smoke CA" -keyout "$WORK/ca.key" -out "$WORK/ca.pem" \
  -addext "basicConstraints=critical,CA:TRUE" -addext "keyUsage=critical,keyCertSign" 2>/dev/null
openssl req -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes -subj "/CN=localhost" \
  -keyout "$WORK/tls/privkey.pem" -out "$WORK/leaf.csr" 2>/dev/null
printf 'subjectAltName=DNS:localhost,IP:127.0.0.1\nextendedKeyUsage=serverAuth\n' > "$WORK/ext.cnf"
openssl x509 -req -in "$WORK/leaf.csr" -CA "$WORK/ca.pem" -CAkey "$WORK/ca.key" -CAcreateserial \
  -days 2 -extfile "$WORK/ext.cnf" -out "$WORK/leaf.pem" 2>/dev/null
cat "$WORK/leaf.pem" "$WORK/ca.pem" > "$WORK/tls/fullchain.pem"
chmod 0644 "$WORK/tls/"*.pem

say "the image runs as a non-root user with one volume"
docker image inspect "$IMAGE" --format 'user={{.Config.User}} volumes={{json .Config.Volumes}} size={{.Size}}'
test "$(docker image inspect "$IMAGE" --format '{{.Config.User}}')" = "65532:65532"

say "start it read-only"
docker run -d --name "$NAME" --read-only --tmpfs /tmp:size=64m --cap-drop ALL \
  --security-opt no-new-privileges:true \
  -v "$VOLUME:/data" -v "$WORK/tls:/run/lockdown/tls:ro" \
  -e LOCKDOWN_TLS_CERT=/run/lockdown/tls/fullchain.pem -e LOCKDOWN_TLS_KEY=/run/lockdown/tls/privkey.pem \
  -e LOCKDOWN_SIGNUPS=open -e LOCKDOWN_BLOB_DIR=/data/blobs \
  -p "127.0.0.1:$PORT:8443" "$IMAGE" >/dev/null
URL="https://localhost:$PORT"
for _ in $(seq 1 60); do
  if curl -fsS --cacert "$WORK/ca.pem" "$URL/healthz" >/dev/null 2>&1; then break; fi
  sleep 1
done
curl -fsS --cacert "$WORK/ca.pem" "$URL/healthz"; echo
curl -fsSI --cacert "$WORK/ca.pem" "$URL/healthz" | grep -i '^strict-transport-security'
curl -fsS --cacert "$WORK/ca.pem" "$URL/.well-known/lockdown"; echo
docker exec "$NAME" /usr/local/bin/lockdown-server healthcheck && echo "healthcheck: ok"
idle_kib() { docker stats --no-stream --format '{{.MemUsage}}' "$NAME"; }
echo "memory, idle after start: $(idle_kib)"

say "register, add, sync with the real CLI over hybrid TLS"
export LOCKDOWN_CA_CERT="$WORK/ca.pem"
A="$WORK/a/snds/identity"; B="$WORK/b/snds/identity"
PW='a long smoke-test password'
HOSTNAME=laptop "$CLI" --root "$A" init
OUT=$(printf '%s\n' "$PW" | HOSTNAME=laptop "$CLI" --root "$A" account register --server "$URL" --login smoke@example.com --password-stdin)
SK=$(printf '%s\n' "$OUT" | awk '/^Secret Key:/ {print $NF}')
RC=$(printf '%s\n' "$OUT" | awk '/^Recovery code:/ {print $NF}')
printf 'first secret\n' | "$CLI" --root "$A" add login --site https://bank.example --username me --password-stdin
"$CLI" --root "$A" sync | tee "$WORK/sync.txt"
grep -q "pushed 1" "$WORK/sync.txt"

say "a second device recovers with the kit and gets the item"
printf '%s\n%s\n%s\n' "$PW" "$SK" "$RC" | HOSTNAME=phone "$CLI" --root "$B" recover --server "$URL" --login smoke@example.com --password-stdin --kit-stdin
"$CLI" --root "$B" list | tee "$WORK/list.txt"
grep -q bank.example "$WORK/list.txt"

say "some load: twenty syncs from each device"
for i in $(seq 1 20); do
  printf 'secret %s\n' "$i" | "$CLI" --root "$A" add login --site "https://site$i.example" --username me --password-stdin >/dev/null
  "$CLI" --root "$A" sync >/dev/null
  "$CLI" --root "$B" sync >/dev/null
done
"$CLI" --root "$B" list | wc -l
echo "memory, after load: $(idle_kib)"

say "a backup inside the container, verified"
KEY=$(docker exec "$NAME" /usr/local/bin/lockdown-server backup keygen 2>/dev/null)
docker exec -e LOCKDOWN_BACKUP_KEY="$KEY" "$NAME" /usr/local/bin/lockdown-server backup create --out /data/smoke.ldbk
docker exec -e LOCKDOWN_BACKUP_KEY="$KEY" "$NAME" /usr/local/bin/lockdown-server backup verify --in /data/smoke.ldbk

say "the root file system really is read-only"
if docker exec -e LOCKDOWN_BACKUP_KEY="$KEY" "$NAME" /usr/local/bin/lockdown-server backup create --out /usr/local/bin/x.ldbk >/dev/null 2>&1; then
  echo "wrote outside /data"; exit 1
fi
echo "refused, as it should be"
say "smoke test passed"

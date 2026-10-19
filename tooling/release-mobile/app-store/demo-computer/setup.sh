#!/bin/sh
set -eu

cd "$(dirname "$0")"
: "${GHOSTEX_DEMO_PASSWORD:?Set GHOSTEX_DEMO_PASSWORD before running setup.sh}"

if ! docker image inspect ghostex-appstore-demo:10.26.0 >/dev/null 2>&1; then
  mkdir -p runtime
  if [ ! -x runtime/bin/ghostex ]; then
    gh release download v10.26.0 --repo maddada/Ghostex \
      --pattern gxserver-linux-arm64.tar.gz --dir runtime --clobber
    tar -xzf runtime/gxserver-linux-arm64.tar.gz -C runtime
    rm runtime/gxserver-linux-arm64.tar.gz
  fi
  docker build --platform linux/arm64 -t ghostex-appstore-demo:10.26.0 .
fi

for spec in ghostex-appstore-demo:2222 ghostex-appstore-home:2223; do
  name=${spec%:*}
  port=${spec#*:}
  if docker container inspect "$name" >/dev/null 2>&1; then
    if [ "$(docker inspect -f '{{.State.Running}}' "$name")" != true ]; then
      docker start "$name" >/dev/null
    fi
  else
    docker run -d --name "$name" -p "127.0.0.1:$port:22" \
      -e GHOSTEX_DEMO_PASSWORD="$GHOSTEX_DEMO_PASSWORD" \
      ghostex-appstore-demo:10.26.0 >/dev/null
  fi
  attempt=0
  until docker exec -u demo "$name" ghostex sessions --json >/dev/null 2>&1; do
    attempt=$((attempt + 1))
    [ "$attempt" -lt 30 ] || { echo "gxserver did not start in $name" >&2; exit 1; }
    sleep 1
  done
  docker exec -u demo "$name" git -C /home/demo/projects/side-project init -q
  docker exec -u demo "$name" ghostex add-project /home/demo/projects/side-project \
    --name 'Side project' --json >/dev/null
  if [ "$name" = ghostex-appstore-demo ]; then
    for project in atlas-api wander-travel; do
      docker exec -u demo "$name" git -C "/home/demo/projects/$project" init -q
    done
    docker exec -u demo "$name" ghostex add-project /home/demo/projects/atlas-api \
      --name 'Atlas API' --json >/dev/null
    docker exec -u demo "$name" ghostex add-project /home/demo/projects/wander-travel \
      --name 'Wander travel' --json >/dev/null
  fi
  echo "Started $name at 127.0.0.1:$port."
done

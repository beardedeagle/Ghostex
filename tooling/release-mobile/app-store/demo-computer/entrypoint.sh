#!/bin/sh
set -eu

: "${GHOSTEX_DEMO_PASSWORD:?Set GHOSTEX_DEMO_PASSWORD for this local demo container}"
printf 'demo:%s\n' "$GHOSTEX_DEMO_PASSWORD" | chpasswd
unset GHOSTEX_DEMO_PASSWORD
ssh-keygen -A >/dev/null

su -s /bin/sh demo -c 'ghostex server start' >/home/demo/gxserver-start.log 2>&1
su -s /bin/sh demo -c 'cd /home/demo/projects/wander-travel/site && nohup python3 -m http.server 3000 --bind 127.0.0.1 >/home/demo/web-preview.log 2>&1 &'

exec /usr/sbin/sshd -D -e

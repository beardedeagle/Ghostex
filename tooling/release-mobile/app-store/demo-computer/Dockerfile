FROM node:22-bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates git iproute2 openssh-server procps python3 && \
    rm -rf /var/lib/apt/lists/* && \
    npm install -g @openai/codex@0.163.0 && \
    useradd --create-home --shell /bin/bash demo && \
    mkdir -p /run/sshd /home/demo/projects /opt/ghostex

COPY runtime/ /opt/ghostex/
COPY projects/ /home/demo/projects/
COPY entrypoint.sh /usr/local/bin/ghostex-demo-entrypoint

RUN chmod +x /usr/local/bin/ghostex-demo-entrypoint /opt/ghostex/bin/* && \
    ln -s /opt/ghostex/bin/ghostex /usr/local/bin/ghostex && \
    ln -s /opt/ghostex/bin/gxserver /usr/local/bin/gxserver && \
    ln -s /opt/ghostex/bin/zmx /usr/local/bin/zmx && \
    chown -R demo:demo /home/demo && \
    printf 'PasswordAuthentication yes\nPermitRootLogin no\n' >/etc/ssh/sshd_config.d/ghostex-demo.conf

EXPOSE 22 3000
ENTRYPOINT ["/usr/local/bin/ghostex-demo-entrypoint"]

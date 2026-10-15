ARG BASE_IMAGE=ubuntu:24.04
FROM ${BASE_IMAGE}

LABEL org.datagrand.rpa.environment="linux-xvfb-v1" \
      org.datagrand.rpa.role="runtime"

USER root
RUN apt-get update \
 && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
      bash ca-certificates coreutils tini util-linux \
      xvfb openbox x11vnc novnc websockify \
      x11-utils x11-xserver-utils xauth python3 python3-tk fonts-dejavu-core \
      libgcc-s1 libstdc++6 libdbus-1-3 \
      libx11-6 libx11-xcb1 libxtst6 libxext6 libxi6 libxfixes3 libxrandr2 \
      libxrender1 libxcursor1 libxinerama1 libxcb1 libxcb-randr0 libxcb-shm0 \
      libxcb-xfixes0 libxcb-shape0 libxcb-render0 libxcb-xinput0 \
      libxkbcommon0 libxkbcommon-x11-0 \
 && rm -rf /var/lib/apt/lists/* \
 && groupadd --gid 10001 rpa \
 && useradd --uid 10001 --gid 10001 --create-home --shell /bin/bash rpa \
 && install -d -m 0700 -o rpa -g rpa /home/rpa/.runtime \
 && install -d -m 0755 -o rpa -g rpa /opt/rpa-host /opt/rpa-fixture

COPY session-env.sh entrypoint.sh healthcheck.sh exec-session.sh /usr/local/bin/
COPY fixture.py /opt/rpa-fixture/fixture.py
RUN chmod 0755 /usr/local/bin/entrypoint.sh /usr/local/bin/healthcheck.sh \
      /usr/local/bin/exec-session.sh \
 && chmod 0644 /usr/local/bin/session-env.sh /opt/rpa-fixture/fixture.py

ENV HOME=/home/rpa \
    USER=rpa \
    DISPLAY=:99 \
    XDG_SESSION_TYPE=x11 \
    XDG_RUNTIME_DIR=/home/rpa/.runtime \
    XAUTHORITY=/home/rpa/.Xauthority \
    SCREEN_GEOMETRY=1280x720x24 \
    ENABLE_FIXTURE=0 \
    LANG=C.UTF-8 \
    LC_ALL=C.UTF-8 \
    PYTHONUNBUFFERED=1
USER 10001:10001
WORKDIR /home/rpa
EXPOSE 6080
HEALTHCHECK --interval=10s --timeout=8s --start-period=45s --retries=3 \
    CMD ["/usr/local/bin/healthcheck.sh"]
ENTRYPOINT ["/usr/bin/tini", "-g", "--", "/usr/local/bin/entrypoint.sh"]
CMD []

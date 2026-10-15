#!/usr/bin/env bash
# Run on the Linux Docker host. No SSH, Compose, provisioning or test execution.
set -euo pipefail

NAME=rpa-linux-xvfb
IMAGE=${IMAGE:-rpa-linux-xvfb:local}
OWNER_KEY=org.datagrand.rpa.managed-by
OWNER_VALUE=linux-xvfb-run-v1

usage() {
    cat <<'USAGE'
Usage: ./run.sh start|resume|status|logs|stop|remove
       ./run.sh exec COMMAND [ARG ...]

start   Create a new container; any existing same-name container is refused.
resume  Start the existing stopped container without recreating it.
status  Show state and health of the label-owned container.
logs    Show its last 200 log lines.
stop    Gracefully stop it; preserve the container and its files.
remove  Remove it only if already stopped; copied files will be lost.
exec    Run a command via exec-session.sh as 10001:10001, stdin open, no TTY.

Name: rpa-linux-xvfb. Image: IMAGE or rpa-linux-xvfb:local.
Observation: 127.0.0.1:16080 -> 6080, view-only. No host/TLS port is published.
USAGE
}
fail() { echo "linux-xvfb: $*" >&2; exit 1; }
owned_container() {
    local record owner
    record=$(docker container inspect --format \
        "{{.Id}} {{index .Config.Labels \"$OWNER_KEY\"}}" "$NAME") \
        || fail "Container $NAME is unavailable; nothing changed."
    read -r container_id owner <<< "$record"
    [[ -n $container_id && $owner == "$OWNER_VALUE" ]] \
        || fail "Container $NAME has no matching ownership label; refusing."
    # Subsequent operations use the inspected immutable ID, not a reusable name.
}

command=${1:-help}
case "$command" in
    help|-h|--help) usage; exit 0 ;;
    start|resume|status|logs|stop|remove)
        (( $# == 1 )) || { usage >&2; exit 2; } ;;
    exec) (( $# >= 2 )) || { usage >&2; exit 2; } ;;
    *) usage >&2; exit 2 ;;
esac
command -v docker >/dev/null || fail 'docker is required on the Linux host.'
[[ $(docker info --format '{{.OSType}}') == linux ]] \
    || fail 'A reachable Linux Docker Engine is required.'

case "$command" in
    start)
        existing=$(docker container ls --all --filter "name=^/${NAME}$" --format '{{.ID}}')
        [[ -z $existing ]] || fail "Container $NAME already exists; inspect it first. No replacement performed."
        image_contract=$(docker image inspect --format \
            '{{index .Config.Labels "org.datagrand.rpa.role"}} {{.Config.User}}' "$IMAGE")
        [[ $image_contract == 'runtime 10001:10001' ]] \
            || fail 'Use the runtime image (user 10001:10001), not the builder.'
        docker run --detach --pull=never \
            --name "$NAME" \
            --label "$OWNER_KEY=$OWNER_VALUE" \
            --user 10001:10001 \
            --network bridge \
            --publish 127.0.0.1:16080:6080/tcp \
            --cpus 4 --memory 4g --memory-swap 4g \
            --shm-size 512m --pids-limit 256 \
            --cap-drop ALL --security-opt no-new-privileges=true \
            --log-driver json-file --log-opt max-size=10m --log-opt max-file=3 \
            --restart no --stop-timeout 10 \
            --env ENABLE_FIXTURE=1 \
            "$IMAGE"
        echo 'Container created; readiness is not yet verified. Use ./run.sh status and logs.'
        ;;
    resume)
        owned_container
        state=$(docker container inspect --format '{{.State.Status}}' "$container_id")
        case "$state" in
            exited)
                docker start "$container_id"
                echo 'Existing container started; readiness is not yet verified. Use ./run.sh status and logs.'
                ;;
            running)
                echo 'Container is already running; no changes made.'
                ;;
            *) fail "Container state is $state, not exited; no changes made." ;;
        esac
        ;;
    status)
        owned_container
        docker container inspect --format \
            'name={{.Name}} state={{.State.Status}} exit={{.State.ExitCode}} health={{if .State.Health}}{{.State.Health.Status}}{{else}}none{{end}}' \
            "$container_id"
        ;;
    logs)
        owned_container
        docker logs --tail 200 "$container_id"
        ;;
    stop)
        owned_container
        docker stop --time 10 "$container_id"
        ;;
    remove)
        owned_container
        [[ $(docker container inspect --format '{{.State.Running}}' "$container_id") == false ]] \
            || fail 'Stop the container before removing it; forced removal is disabled.'
        docker container rm "$container_id"
        ;;
    exec)
        owned_container
        shift
        exec docker exec --interactive --user 10001:10001 "$container_id" \
            /usr/local/bin/exec-session.sh "$@"
        ;;
esac

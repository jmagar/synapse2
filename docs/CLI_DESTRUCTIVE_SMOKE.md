# Destructive CLI Smoke Route

This route validates mutating `synapse flux` commands against disposable Docker
resources. It is intended for local operator validation, not CI.

## Safety Model

- Use unique names with a timestamp prefix: `synapse-cli-<epoch>`.
- Label every created Docker resource with `synapse.test=<id>`.
- Run all commands with `--host local` unless intentionally validating a remote
  host.
- Clean up by name/label at the end.
- Do not run unscoped prune targets when unrelated resources are present.

Docker prune APIs used by Synapse are not label-scoped:

- `docker prune --target containers` removes all stopped containers.
- `docker prune --target images` removes all dangling images.
- `docker prune --target networks` removes all unused networks.
- `docker prune --target volumes` removes all dangling volumes.
- `docker prune --target buildcache` removes build cache globally.
- `docker prune --target all` combines broad prune targets.

Before running prune commands, check for pre-existing resources:

```bash
docker ps -a --filter status=exited --filter status=created --filter status=dead \
  --format '{{.ID}} {{.Names}} {{.Image}}'
docker image ls --filter dangling=true --format '{{.ID}} {{.Repository}}:{{.Tag}}'
docker volume ls --filter dangling=true --format '{{.Name}}'
docker network ls --filter dangling=true --format '{{.ID}} {{.Name}}'
```

If pre-existing resources exist, skip that prune target unless the operator has
explicitly approved removing them.

## Disposable Resources

Create a temporary build context and compose stack:

```bash
ID="synapse-cli-$(date +%s)"
ROOT="/tmp/$ID"
CTX="$ROOT/buildctx"
COMPOSE_DIR="$ROOT/composeproj"
mkdir -p "$CTX" "$COMPOSE_DIR"

printf 'FROM busybox:latest\nLABEL synapse.test=%s\nCMD ["sh", "-c", "while true; do sleep 1; done"]\n' \
  "$ID" > "$CTX/Dockerfile"

printf 'name: %s\nservices:\n  app:\n    image: busybox:latest\n    command: ["sh", "-c", "while true; do sleep 1; done"]\n    labels:\n      synapse.test: %s\n  built:\n    build:\n      context: ../buildctx\n    image: %s-compose-built:latest\n    command: ["sh", "-c", "while true; do sleep 1; done"]\n    labels:\n      synapse.test: %s\n' \
  "$ID" "$ID" "$ID" "$ID" > "$COMPOSE_DIR/compose.yml"

export SYNAPSE_HOSTS_CONFIG="[{\"name\":\"local\",\"host\":\"localhost\",\"protocol\":\"local\",\"dockerSocketPath\":\"/var/run/docker.sock\",\"composeSearchPaths\":[\"$ROOT\"]}]"
```

Cleanup:

```bash
docker rm -f "$ID-container" "$ID-prune-container" 2>/dev/null || true
docker compose -f "$COMPOSE_DIR/compose.yml" down --volumes --remove-orphans 2>/dev/null || true
docker image rm -f "$ID-built:latest" "$ID-compose-built:latest" "$ID-dangling:latest" 2>/dev/null || true
docker network rm "$ID-net" 2>/dev/null || true
rm -rf "$ROOT"
```

## Commands To Run

Docker image/build/remove:

```bash
synapse flux docker pull --host local --image busybox:latest
synapse flux docker build --host local --context "$CTX" --tag "$ID-built:latest"
```

Create the disposable container:

```bash
docker run -d --name "$ID-container" --label synapse.test="$ID" "$ID-built:latest"
CID="$(docker ps -q --filter "name=^/${ID}-container$")"
```

Container lifecycle:

```bash
synapse flux container exec --host local --container-id "$CID" --command sh -c 'echo synapse-ok'
synapse flux container restart --host local --container-id "$CID"
synapse flux container pause --host local --container-id "$CID"
synapse flux container resume --host local --container-id "$CID"
synapse flux container stop --host local --container-id "$CID"
synapse flux container start --host local --container-id "$CID"
synapse flux container recreate --host local --container-id "$CID" --no-pull
CID="$(docker ps -q --filter "name=^/${ID}-container$")"
synapse flux container exec --host local --container-id "$CID" --command sh -c 'echo synapse-recreated'
```

`container pull` pulls the inspected container image. Use a container created
from a registry-backed image such as `busybox:latest`, not a local-only test
image tag:

```bash
docker run -d --name "$ID-pull-container" --label synapse.test="$ID" busybox:latest sh -c 'while true; do sleep 1; done'
PULL_CID="$(docker ps -q --filter "name=^/${ID}-pull-container$")"
synapse flux container pull --host local --container-id "$PULL_CID"
docker rm -f "$ID-pull-container"
```

Compose stack:

```bash
synapse flux compose refresh --host local
synapse flux compose list --host local
synapse flux compose pull --host local --project "$ID" --service app
synapse flux compose build --host local --project "$ID" --service built
synapse flux compose up --host local --project "$ID"
synapse flux compose status --host local --project "$ID"
synapse flux compose logs --host local --project "$ID" --lines 5
synapse flux compose restart --host local --project "$ID"
synapse flux compose recreate --host local --project "$ID"
synapse flux compose down --host local --project "$ID" --force
```

Prune and remove:

```bash
docker create --name "$ID-prune-container" --label synapse.test="$ID" busybox:latest true
synapse flux docker prune --host local --target containers --force

synapse flux docker rmi --host local --image "$ID-built:latest" --force

# Only when no unrelated dangling images are present.
printf 'FROM busybox:latest\nLABEL synapse.test=%s\n' "$ID" > "$CTX/Dockerfile"
docker build -q -t "$ID-dangling:latest" "$CTX" >/dev/null
printf 'FROM busybox:latest\nLABEL synapse.test=%s-replaced\n' "$ID" > "$CTX/Dockerfile"
docker build -q -t "$ID-dangling:latest" "$CTX" >/dev/null
synapse flux docker prune --host local --target images --force

# Only when no unrelated unused networks are present.
docker network create --label synapse.test="$ID" "$ID-net"
synapse flux docker prune --host local --target networks --force
```

Skip these by default unless the operator explicitly approves broad cleanup:

```bash
synapse flux docker prune --host local --target volumes --force
synapse flux docker prune --host local --target buildcache --force
synapse flux docker prune --host local --target all --force
```

## Findings From The First Route Run

- `docker pull`, `docker build`, container `restart/pause/resume/stop/start/recreate`,
  compose `refresh/list/pull/build/up/status/logs/restart/recreate/down`,
  `docker prune --target containers`, `docker prune --target networks`, and
  `docker rmi` passed against disposable resources.
- `container exec --command sh -c ...` exposed a CLI parser bug: tokens after
  `--command` must be treated as exec argv, not normal CLI flags.
  Current status: fixed by treating every token after `--command` as container
  argv, including tokens that look like Synapse flags.
- `container pull` failed against a local-only built image because Docker tried
  to pull that image name from a registry. Use a registry-backed image container
  for that subaction.

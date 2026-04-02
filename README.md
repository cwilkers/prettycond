# prettycond

`prettycond` reads a Kubernetes-style JSON document from **standard input** and prints **status conditions** as a fixed-width table: type, status, reason, and last transition time as a human-readable age (for example `13m ago`).

The tool is a small Rust binary with fast startup, which makes it comfortable to run repeatedly in shell pipelines.

## Requirements

- [Rust](https://www.rust-lang.org/tools/install) (stable) and `cargo`

## Build

From this directory:

```bash
# same as make all — release binary
$ make

# to get help on the Makefile targets, run
$ make help
```

Or without Make:

```bash
cargo build --release
```

The binary is `target/release/prettycond`. You can install it into your Cargo bin path with:

```bash
cargo install --path .
```

## Usage

Pipe JSON for a single CR into `prettycond`:

```bash
$ oc get pod mypod -o json | prettycond

TYPE                       STATUS  REASON  LAST_TRANSITION
ContainersReady            True    -       29h ago        
Initialized                True    -       29h ago        
PodReadyToStartContainers  True    -       29h ago        
PodScheduled               True    -       29h ago        
Ready                      True    -       29h ago        
```

The default JSON path is `status.conditions` (a dot-separated path to an array of condition objects, or a single object).
If the object you want to inspect has its conditions in a different path, you can alter them with the `--path` option.

### Multiple resources (`kubectl get … -o json`)

If the document is a Kubernetes **List** (`kind: List` with an `items` array), each element is handled separately. Output is one **stanza** per resource: a title line `Kind namespace/name` (or `Kind name` when there is no namespace), a blank line before every stanza after the first, then the usual condition table for that object.

Resources are ordered by **`metadata.namespace` then `metadata.name`** (empty namespace sorts first, for cluster-scoped objects). Sort flags (`-s`, `-t`, `-U`, default type sort, and `-r`) apply **only to the condition rows within each resource**, not to the list of resources.

```bash
kubectl get pods -n myns -o json | prettycond
```

```bash
$ kubectl get myCR example -o json | prettycond --path status.customConditions

ValidConfiguration               True     AsExpected                   9h ago         
ValidImage                       True     AsExpected                   9h ago         
ValidInfo                        True     AsExpected                   9h ago         

```

### Options

| Flag | Description |
|------|-------------|
| `--path <PATH>` | JSON path to the conditions list (default: `status.conditions`) |
| `--no-header` | Omit the column header row |
| `-s` / `--sort-status` | Sort by status (then type, reason) |
| `-t` / `--sort-time` | Sort by `lastTransitionTime`, most recent first |
| `-U` / `--unsorted` | Keep the order of entries in the JSON array |
| *(default)* | Sort by **type** |
| `-r` / `--reverse` | Reverse the current sort order (no effect with `-U`) |

Only one of `-U`, `-s`, and `-t` may be used at a time. `-r` combines with the active sort mode.

Path segments are simple dot-separated keys (for example `status.someGroup.conditions`), not full [JSONPath](https://kubernetes.io/docs/reference/kubectl/jsonpath/) expressions.

## Condition shape

Each entry is expected to be a JSON object with optional fields such as `type`, `status`, `reason`, and `lastTransitionTime` (RFC3339). Missing string fields are shown as `-`. Rows that are not JSON objects are skipped, with a warning on standard error.

## Attribution

Substantial parts of this project (design, Rust implementation, tests, build wiring, and documentation) were written with assistance from the **Cursor** IDE and its **AI coding agent** ([cursor.com](https://cursor.com)). Human maintainers remain responsible for the shipped code.

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) or <https://www.apache.org/licenses/LICENSE-2.0>.

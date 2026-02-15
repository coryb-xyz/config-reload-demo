# config-reload-demo

A demo application that demonstrates Helm's **automatic deployment rolling** pattern. When a ConfigMap or Secret changes, running pods automatically restart to pick up new values -- without any manual intervention.

## How It Works

The Helm chart includes `sha256sum` annotations on the Deployment's pod template:

```yaml
annotations:
  checksum/config: {{ include (print $.Template.BasePath "/configmap.yaml") . | sha256sum }}
  checksum/secret: {{ include (print $.Template.BasePath "/secret.yaml") . | sha256sum }}
```

When `helm upgrade` is run and a ConfigMap or Secret value has changed, the rendered checksum changes, Kubernetes sees a pod template spec change, and triggers a rolling update. No external controllers or manual restarts required.

This is the pattern described in the Helm documentation: [Automatically Roll Deployments](https://helm.sh/docs/howto/charts_tips_and_tricks/#automatically-roll-deployments).

## The Application

A minimal Rust/Axum HTTP server that reads configuration from environment variables at startup and renders a single HTML page displaying all of them. This makes it visually obvious when a rollout has occurred after a config or secret change.

The app does **not** hot-reload config on its own. The restart _is_ the reload mechanism.

### Environment Variables

**From ConfigMap:** `APP_NAME`, `ENVIRONMENT`, `LOG_LEVEL`, `FEATURE_FLAGS`

**From Secret:** `DATABASE_URL`, `API_KEY`

## Quick Start

```bash
# Run locally
cargo run

# Build and run with Docker
docker build -t config-reload-demo:latest .
docker run -p 8080:8080 \
  -e APP_NAME=config-reload-demo \
  -e ENVIRONMENT=local \
  -e LOG_LEVEL=debug \
  -e FEATURE_FLAGS=dark-mode \
  -e DATABASE_URL=postgres://local:local@localhost/demo \
  -e API_KEY=sk-local-testing \
  config-reload-demo:latest
```

## Helm Chart

### Install

```bash
helm install my-release oci://ghcr.io/coryb-xyz/config-reload-demo --version 0.1.0
```

### Verify the Auto-Roll Pattern

The key validation: changing a config value should change the checksum annotation and nothing else.

```bash
diff \
  <(helm template rel chart/config-reload-demo/ --set config.logLevel=info) \
  <(helm template rel chart/config-reload-demo/ --set config.logLevel=debug)
```

### Resource Naming

By default, the chart uses a standard Helm naming convention: `{release-name}-{chart-name}`. If the release name already contains the chart name, it avoids duplication.

You can control resource names with two values:

| Value | Effect |
|---|---|
| `nameOverride` | Replaces the chart name portion (affects the `app.kubernetes.io/name` label and the fallback in fullname) |
| `fullnameOverride` | Replaces the entire computed resource name directly |

**Examples:**

```bash
# Default: release name = "my-release"
# Resources are named: my-release-config-reload-demo
helm install my-release chart/config-reload-demo/

# Release name matches chart name -- deduplication kicks in
# Resources are named: config-reload-demo (not config-reload-demo-config-reload-demo)
helm install config-reload-demo chart/config-reload-demo/

# fullnameOverride: all resources use exactly this name
# Resources are named: my-app
helm install whatever chart/config-reload-demo/ --set fullnameOverride=my-app

# nameOverride: replaces the chart name portion
# Resources are named: my-release-my-app
helm install my-release chart/config-reload-demo/ --set nameOverride=my-app
```

For a concrete example, `helm install my-release chart/config-reload-demo/ --set fullnameOverride=demo` creates:

| Kind | Name |
|---|---|
| Deployment | `demo` |
| Service | `demo` |
| ConfigMap | `demo` |
| Secret | `demo` |

All resources share the same fullname, which is how they reference each other (e.g., the Deployment's `envFrom` points to the ConfigMap and Secret by this name).

### Values Schema

The chart includes a `values.schema.json` that validates values on `helm install`, `helm upgrade`, `helm lint`, and `helm template`. This catches misconfiguration early -- before anything reaches a cluster.

See [Schema Files](https://helm.sh/docs/topics/charts/#schema-files) in the Helm documentation.

### Secret Handling

The chart supports two modes via `secrets.create`:

- **`true`** (default): The chart renders its own Secret from values. The sha256sum annotation triggers rollouts on secret changes.
- **`false`**: Secrets are managed externally (e.g., SOPS + FluxCD). The checksum annotation is set to a static value.

#### GitOps with SOPS: Keeping Helm-Native Auto-Roll

In a GitOps setup, secrets are typically managed outside the Helm chart as SOPS-encrypted manifests. The naive approach is to set `secrets.create: false` and let an external controller handle the Secret -- but this breaks the sha256sum rollout mechanism, requiring a separate tool like [Stakater Reloader](https://github.com/stakater/Reloader) to watch for Secret changes.

A cleaner approach is to keep `secrets.create: true` and feed the decrypted secret values _through_ Helm's values pipeline. With FluxCD, this is done using `valuesFrom` on the `HelmRelease`:

**1. Create a SOPS-encrypted "feeder" Secret in the GitOps repo:**

```yaml
# secrets/config-reload-demo-secrets.enc.yaml
apiVersion: v1
kind: Secret
metadata:
  name: config-reload-demo-secrets
stringData:
  databaseUrl: postgres://prod:realpass@rds-host/proddb
  apiKey: sk-prod-real-key
```

Encrypt with SOPS and commit. FluxCD's kustomize-controller decrypts this at apply time.

**2. Reference the feeder Secret in the HelmRelease:**

```yaml
# helmrelease.yaml
apiVersion: helm.toolkit.fluxcd.io/v2
kind: HelmRelease
metadata:
  name: config-reload-demo
spec:
  chart:
    spec:
      chart: config-reload-demo
      version: "0.1.0"
      sourceRef:
        kind: OCIRepository
        name: config-reload-demo
  values:
    config:
      environment: production
      logLevel: warn
    secrets:
      create: true   # chart still renders the Secret
  valuesFrom:
    - kind: Secret
      name: config-reload-demo-secrets
      valuesKey: databaseUrl
      targetPath: secrets.databaseUrl
    - kind: Secret
      name: config-reload-demo-secrets
      valuesKey: apiKey
      targetPath: secrets.apiKey
```

**How it works:** Flux decrypts the SOPS secret, applies it to the cluster, then the `HelmRelease` reads those values via `valuesFrom` and injects them into `.Values.secrets.*`. The chart renders its own Secret as usual, the sha256sum annotation hashes the rendered content, and any secret change triggers a rolling restart -- entirely through Helm's native mechanism.

The tradeoff is two Secret objects in the cluster (the feeder and the chart-rendered one), but the benefit is a single, unified rollout mechanism for both ConfigMap and Secret changes with no extra controllers.

See [HelmRelease `valuesFrom`](https://fluxcd.io/flux/components/helm/helmreleases/#values-overrides) in the FluxCD documentation.

## Relevant Helm Documentation

- [Automatically Roll Deployments](https://helm.sh/docs/howto/charts_tips_and_tricks/#automatically-roll-deployments) -- the core pattern this project demonstrates
- [Schema Files](https://helm.sh/docs/topics/charts/#schema-files) -- values validation via JSON Schema
- [Helm OCI Support](https://helm.sh/docs/topics/registries/) -- the chart is published as an OCI artifact to GHCR

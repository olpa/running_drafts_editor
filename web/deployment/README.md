# Running Drafts frontend deployment

This directory supplies the host scripts installed at
`/rnotes/RUNNING_DRAFTS_FRONTEND`. The image source and parameterized Compose
file remain in `/mnt/HC_Volume_103849597/home/olpa/running_drafts_editor/web`.
The source repository is `olpa/running_drafts_editor`; the host operator owns
this deployment.

Build a tagged image from `web/`:

```sh
IMAGE_TAG=0.1.0 make image
```

Each target selects its deployed tag only through `.env.dev` or `.env.prod`.
For a normal rollout, build the tag named in that file and deploy it:

```sh
/rnotes/RUNNING_DRAFTS_FRONTEND/deploy-dev.sh
/rnotes/RUNNING_DRAFTS_FRONTEND/status-dev.sh
```

Development uses Compose project `running-drafts-frontend-dev`, loopback HTTP
port `18083`, planned hostname `drafts-dev.handsfree.vc`, and backend
`https://drafts-api-dev.handsfree.vc`. Production uses project
`running-drafts-frontend-prod`, loopback HTTP port `18084`, planned hostname
`drafts.handsfree.vc`, and backend `https://drafts-api.handsfree.vc`. The
browser-visible backend URL is generated at container startup; it is not a
secret. The frontend has no durable mutable data, migrations, or backups.

Public boringproxy routes are not part of these scripts. Before adding a route,
verify DNS, back up the boringproxy database, and use server TLS termination to
the target's loopback HTTP port. Preserve all existing routes. Certificate
issuance and renewal belong to boringproxy; recreating its route reloads the
certificate. Do not call either target publicly deployed until its route and
public checks have been verified.

Use `status-dev.sh` or `status-prod.sh` for local liveness. Pass normal Compose
log options to `logs-dev.sh` or `logs-prod.sh`. The application health controls
then verify the corresponding backend's `/health/live` and `/health/ready`.

To roll back, set the prior immutable `IMAGE_TAG` in the target environment
file and run its deploy script. Because the frontend owns no data, rollback has
no data-compatibility constraint. Retain the prior image until the new version
has passed local and public checks.

After a route exists, verify in this order:

```sh
/rnotes/RUNNING_DRAFTS_FRONTEND/status-dev.sh
curl --fail https://drafts-dev.handsfree.vc/health/live
curl --fail https://drafts-dev.handsfree.vc/config.js
curl --fail https://drafts-api-dev.handsfree.vc/health/live
curl --fail https://drafts-api-dev.handsfree.vc/health/ready
```

Use the corresponding production script and hostnames for production. After a
controlled container restart, repeat the frontend liveness and `config.js`
checks. Record each rollout's date, source revision, image tag, and resolved
image digest in the runtime README without treating that history as current
configuration.

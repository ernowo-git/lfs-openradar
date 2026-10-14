# Issue tracker: self-hosted Plane

Tickets and specs live in the Race Hub project (RAC) in workspace `sampingan`.

- Instance: https://plane.ctrlchsr.space/
- Project ID: `4f36a711-f98d-48a3-9227-5d674915f6d4`
- Project URL: https://plane.ctrlchsr.space/sampingan/projects/4f36a711-f98d-48a3-9227-5d674915f6d4/issues/
- Project API base: `https://plane.ctrlchsr.space/api/v1/workspaces/sampingan/projects/4f36a711-f98d-48a3-9227-5d674915f6d4/`

## Access

Use this instance's REST API. Authenticate with the `X-API-Key` header, taking the token from `PLANE_API_KEY` supplied at runtime. Never commit or print the token. Send `Accept: application/json` and `User-Agent: lfs-openradar-agent-setup/1.0`; JSON writes also need `Content-Type: application/json`.

Use a Plane connector only after verifying that it targets this instance and workspace.

## Ticket operations

Paths below are relative to the project API base.

- List: `GET work-items/`. Follow `next_cursor` while `next_page_results` is true.
- Read: `GET work-items/<uuid>/`.
- Create: `POST work-items/` with `name` and `description_html`.
- Update: `PATCH work-items/<uuid>/` with only the fields being changed.
- Labels: `GET labels/`; create missing labels with `POST labels/` using `name` and `color`. Resolve names to UUIDs before applying them. Read and preserve existing ticket labels when updating its `labels` array.
- States: `GET states/`; resolve names to UUIDs before updating a ticket's `state`.
- Complete: set the ticket's state to Done (group `completed`). For work declined during triage, apply `wontfix` and set Cancelled (group `cancelled`).

Refer to tickets as `RAC-<sequence_id>` and keep their UUIDs for API calls. Publishing to the issue tracker means creating a Plane ticket; fetching a relevant ticket means reading its Plane record.

API reference: https://developers.plane.so/api-reference/introduction

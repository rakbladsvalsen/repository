# Map Repository

[![Continuous integration](https://github.com/rakbladsvalsen/repository/actions/workflows/ci.yml/badge.svg)](https://github.com/rakbladsvalsen/repository/actions/workflows/ci.yml)

This app provides a nice and comfy place for all your hashtable-like JSON data.

## Get started

You'll need to set up a Postgres/Postgres-compatible database before using this app. This is outside the scope of this README, though. 

This app provides some knobs that allow you to tune and configure it via environment variables:

| Variable                             | Required? |  Description                                                                                                           |
|--------------------------------------|-----------|------------------------------------------------------------------------------------------------------------------------|
| `HOST`                               | **Yes**   | Listening address, i.e. `127.0.0.1`                                                                                    |
| `PORT`                               | **Yes**   | Listening port, i.e. `8080`                                                                                            |
| `DATABASE_URL`                       | **Yes**   | Postgres database credentials, i.e. `postgres://USERNAME:PASSWORD@IP_ADDRESS:HOST/DATABASE`                            |
| `ED25519_SIGNING_KEY¹`               | **Yes**   | Ed25519 private key (used to sign JWT tokens)                                                                          |
| `TOKEN_EXPIRATION_SECONDS`           | No        | JWT token expiration (in seconds). Set to `5` minutes by default.                                                      |
| `DB_POOL_MIN_CONN`                   | No        | Minimum limit of connections for the database threadpool. Set to `10` by default.                                      |
| `DB_POOL_MAX_CONN`                   | No        | Maximum limit of connections for the database threadpool. Set to `100` by default.                                     |
| `BULK_INSERT_CHUNK_SIZE`             | No        | Create batch insert jobs with `N` entries at most. Set to `250` by default.                                            |
| `PROTECT_SUPERUSER`                  | No        | Prevent CRUD operations against superusers. Set to `true` by default.                                                  |
| `MAX_PAGINATION_SIZE`                | No        | Max pagination size that can be requested by any user. Set to `1000` by default.                                       |
| `DEFAULT_PAGINATION_SIZE`            | No        | Default pagination size. Set to `1000` by default.                                                                     |
| `WORKERS`                            | No        | Sets number of workers to start (per bind address). Set to `16` by default.                                            |
| `RETURN_QUERY_COUNT`                 | No        | Whether to return or not item and page counts for all queries. Set to `true` by default.                               |
| `MAX_JSON_PAYLOAD_SIZE`              | No        | Max JSON payload size for any incoming request. Set to `100000` (100kB) by default.                                    |
| `DB_ACQUIRE_CONNECTION_TIMEOUT_SEC`  | No        | Acquire connection timeout (in seconds). Set to `30`s by default.                                                      |
| `DB_CSV_STREAM_WORKERS`              | No        | N# of database streams (and workers) to use when streaming DB data. Set to `1` by default.                             |
| `DB_CSV_TRANSFORM_WORKERS`           | No        | N# of workers to use to process the DB stream data. Set to `2` by default.                                             |
| `DB_CSV_WORKER_QUEUE_DEPTH`          | No        | Max N# of items to put in the worker queue for CSV downloads. Set to `200` by default.                                 |
| `MAX_API_KEYS_PER_USER`              | No        | Max N# of API Keys per user. Set to `10` by default.                                                                   |
| `TOKEN_API_KEY_EXPIRATION_HOURS`     | No        | API Key duration, in hours. Set to `720` hours (30 days) by default.                                                   |
| `DB_MAX_STREAMS_PER_USER`            | No        | Max N# of CSV stream connections per user. Set to `2` by default                                                       |
| `TEMPORAL_DELETE_HOURS`              | No        | Allow non-superusers with `limitedDelete` permission to delete records from the last N# hours. Set to `24` by default. |
| `ENABLE_PRUNE_JOB`                   | No        | Whether or not to enable the periodic prune job. This clears old upload sessions. Set to `true` by default.            |
| `PRUNE_JOB_RUN_INTERVAL_SECONDS`     | No        | Run the prune job every N seconds. Set to `600`s (10 min) by default.                                                  |
| `PRUNE_JOB_TIMEOUT_SECONDS`          | No        | Kill the prune job after this many seconds. Set to `300`s (5 min) by default.                                         |


Note ¹: This key can be generated with openssl:
```bash
openssl genpkey -algorithm ED25519
# -----BEGIN PRIVATE KEY-----
#        <Ed25519 KEY>
# -----END PRIVATE KEY-----
```

You only need to set  `<Ed25519 Key>`; the delimiters (`BEGIN...`, `END...`) can be ignored. Only Ed25519 private keys are supported for the time being; you can't use secp*/RSA keys.

For added convenience, you can set all these variables in a `.env` file. It'll be automatically picked up by the app.

## Build

Use the usual cargo commands to build and run both debug and release versions:

```bash
# debug version
cargo run

# optimized version
cargo run --release
```

All necessary tables will be created when this app runs for the first time. **There's no default admin user**, you have to go to the database and create it manually. Passwords are stored in the Argon2 format, so you can use something like `$argon2i$v=19$m=16,t=2,p=1$MTMxMjMxMjMxMjM$C6QFxM2V7P4dKCm/lwAByA` if you want
the password to be `admin`. Use an online argon2 generator to create a different one.

## Logging

This app uses the excellent `log` crate, so you can basically just use:

```
RUST_LOG=debug cargo run --release
```

to log basically everything. If you just want to see this app's messages, use `RUST_LOG=central_repository=debug`.

## Project structure

``` 
├── api
│   └── src: Endpoints and app logic
│       ├── auth: Anything related to auth (JWT tokens and passwords)
│       ├── core_middleware: App middleware (mostly logging and auth)
├── core: DAO layer (DB queries and mutations)
├── entity: DB model definitions
├── macros: Utility macros
├── migration: Migration files. These are executed on app startup.
├── src
│   └── main.rs: app entrypoint

```

## License

LGPL
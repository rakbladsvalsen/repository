# Map Repository

[![Continuous integration](https://github.com/rakbladsvalsen/repository/actions/workflows/ci.yml/badge.svg)](https://github.com/rakbladsvalsen/repository/actions/workflows/ci.yml)

This app provides a nice and comfy place for all your hashtable-like JSON data.

## Get started

You'll need to set up a Postgres/Postgres-compatible database before using this app. This is outside the scope of this README, though. 

This app provides some knobs that allow you to tune and configure it via environment variables:

| Variable                       | Required? |  Description                                                                                 |
|--------------------------------|-----------|----------------------------------------------------------------------------------------------|
| `HOST`                         | **Yes**   | Listening address, i.e. `127.0.0.1`                                                          |
| `PORT`                         | **Yes**   | Listening port, i.e. `8080`                                                                  |
| `DATABASE_URL`                 | **Yes**   | Postgres database credentials, i.e. `postgres://USERNAME:PASSWORD@IP_ADDRESS:HOST/DATABASE`  |
| `ED25519_SIGNING_KEY¹`         | **Yes**   | Ed25519 private key (used to sign JWT tokens)                                                |
| `TOKEN_EXPIRATION_SECONDS`     | No        | JWT token expiration (in seconds). Set to `5` minutes by default.                            |
| `DB_POOL_MIN_CONN`             | No        | Minimum limit of connections for the database threadpool. Set to `10` by default.            |
| `DB_POOL_MAX_CONN`             | No        | Maximum limit of connections for the database threadpool. Set to `100` by default.           |
| `BULK_INSERT_CHUNK_SIZE`       | No        | Create batch insert jobs with `N` entries at most. Set to `250` by default.                  |
| `PROTECT_SUPERUSER`            | No        | Prevent CRUD operations against superusers. Set to `true` by default.                        |

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
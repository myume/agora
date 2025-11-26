# Agora

A reverse proxy (load balancer in the future?) written in rust.

## Usage

You can start the server by running

```bash
cargo run start -- --config <config_path>
```

## Building

The only dependencies you need is a rust compiler and cargo.

You can build the application with

```bash
cargo build --release
```

## Configuration

The configuration file is just a json file. It is a mapping of the prefix of the
path to proxy to a `ProxyEntry`.

```json
{
  "/proxy": {
    "addr": "localhost:3000",
    "strip_prefix": true
  }
}
```

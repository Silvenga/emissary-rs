# Emissary

Emissary bridges the gap between Docker and Consul by automatically registering and deregistering services based on
container lifecycle events. Written in Rust, Emissary is designed to be highly performant and resilient.

A Rust rewrite of my [Emissary](https://github.com/silvenga/emissary) project.

## Features

- **Label Driven:** The service definition lives with your containers, define your Consul services using Docker labels.
- **Event-Based Lifecycle:** Designed to minimize the Docker daemon and Consul agent load, Emissary uses subscriptions
  when possible and automatically registers and deregisters services.
- **Anti-Entropy:** Ensures consistency by periodically syncing with Docker and Consul, even under heavy load or
  networking issues.
- **Lightweight:** Written in Rust, Emissary benefits from native performance and low resource usage.

## Usage

Emissary is shipped as a docker container:

```
docker run \
    --restart always \
    --volume /var/run/docker.sock:/var/run/docker.sock \
    --net host \
    ghcr.io/silvenga/emissary-rs:latest
```

Configure using environment variables or command line flags:

```
Usage: emissary [OPTIONS]

Options:
      --docker-host <DOCKER_HOST>
          Docker host URI [env: DOCKER_HOST=] [default: unix:///var/run/docker.sock]
      --docker-timeout <DOCKER_TIMEOUT>
          Timeout for Docker API requests in seconds [env: DOCKER_TIMEOUT=] [default: 120]
      --consul-host <CONSUL_HOST>
          Consul host address [env: CONSUL_HOST=] [default: http://localhost:8500]
      --consul-timeout <CONSUL_TIMEOUT>
          Timeout for Consul API requests in seconds [env: CONSUL_TIMEOUT=] [default: 3]
      --consul-token <CONSUL_TOKEN>
          Consul ACL token [env: CONSUL_TOKEN=]
      --consul-datacenter <CONSUL_DATACENTER>
          Consul datacenter [env: CONSUL_DATACENTER=]
      --consul-ttl-interval <CONSUL_TTL_INTERVAL>
          Consul TTL interval in seconds [env: CONSUL_TTL_INTERVAL=] [default: 15]
      --consul-start-healthy
          Whether a container in 'starting' state should be considered healthy [env: CONSUL_START_HEALTHY=]
      --polling-interval <POLLING_INTERVAL>
          Polling interval in seconds [env: POLLING_INTERVAL=] [default: 60]
  -h, --help
          Print help
  -V, --version
          Print version
```

## Architecture

The project follows a controller-based actor model implemented using `actix`.

```mermaid
graph LR
    subgraph App
        DockerSupervisor[Docker Supervisor]
    end

    subgraph Docker Side
        DockerSupervisor --> ContainerActor1[Container Actor]
        DockerSupervisor --> ContainerActorN[Container Actor]
    end

    subgraph Consul Side
        ContainerActor1 --> ServiceActor1[Service Actor]
        ContainerActorN --> ServiceActorN[Service Actor]
    end

    DockerDaemon[Docker Daemon] -- Events/Poll --> DockerSupervisor
    ServiceActor1 -- Register/Heartbeat --> ConsulAgent[Consul Agent]
    ServiceActorN -- Register/Heartbeat --> ConsulAgent[Consul Agent]
```

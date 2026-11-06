# Optional local GeoServer/PostGIS example

The [Compose file](docker-compose.yml) supplies a standalone GeoServer/PostGIS environment for manual integration experiments. It is not required by the unit or GPU fixture tests, and starting it does not provision Olayer layers/workspaces or import map datasets automatically.

From the repository root:

```sh
docker compose -f docs/examples/geoserver/docker-compose.yml up -d
```

GeoServer is exposed on `http://localhost:8080`; PostgreSQL on port 5432. Image versions, local example credentials, health checks and named persistent volumes are declared in the Compose file. They are retained from the original example; no live-service compatibility verification is implied by this documentation reorganization.

Stop the example while retaining its named volumes:

```sh
docker compose -f docs/examples/geoserver/docker-compose.yml down
```

Configure the service's workspaces/data and use the [map guides](../../guides/maps/README.md) to point an SDK source at the resulting endpoints. For deterministic verification, use [local fixture tests](../../guides/testing.md#gpu-integration-checks) instead.

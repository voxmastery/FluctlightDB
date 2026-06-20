# Migration v3 → v4

v3 single file: `brain.flct` (bincode + CRC32 header).

v4 directory layout:

```
brain/
  manifest.json
  *.seg
```

Online migration:

```bash
fluctlight compact --path ~/.fluctlight/serverbrain.flct
# future: fluctlight migrate-v4 --path ~/.fluctlight/serverbrain.flct --out ~/.fluctlight/tenants/default/brain_v4
```

Library API: `manifest::migrate_v3_file_to_v4(v3_path, v4_dir)`.

Keep v3 backup until v4 validated.

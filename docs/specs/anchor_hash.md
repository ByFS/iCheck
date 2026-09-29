---
created: 2026-09-24T15:55:00+08:00
updated: 2026-09-26T16:04:00+08:00
---

## anchor_hash

- 覆盖全部文件, 包括上游 API 没提供 Hash 的
- 不存放上游 API 来源之类的字段
- 只记录完全属于文件的 BLAKE3

```json
{
    "tool": "icheck 0.1.0",
    "computed_at": "2026-09-25T13:40:00+08:00",
    "files": [
        {
            "name": "model.safetensors.index.json",
            "size": 7470294,
            "blake3": "e37a9ca5191ee26c061c856c83ca0dcf27a98c2d792c6091baa5a9c2e9e4e909"
        },
        {
            "name": "model-00001-of-00048.safetensors",
            "size": 970533624,
            "blake3": "f03ee0b7a016781235e0c135f16710f54d2492d3f2a1ea0a6f7bad857fc41c4f"
        }
    ]
}
```

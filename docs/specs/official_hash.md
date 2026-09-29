---
created: 2026-09-26T15:55:00+08:00
updated: 2026-09-29T10:04:00+08:00
---

# official_hash

- 只存放上游 API 可以获取的 Hash
- 上游 API 获取不到的不应该出现在结构里
- 不应该出现 null 字段

示例数据

```json
{
    "tool": "icheck 0.1.0",
    "fetched_at": "2026-09-25T01:20:00+08:00",
    "source": {
        "platform": "modelscope",
        "model_id": "deepseek-ai/DeepSeek-V4.1-Flash",
        "url": "https://modelscope.cn/models/deepseek-ai/DeepSeek-V4.1-Flash"
    },
    "files": [
        {
            "name": "model.safetensors.index.json",
            "size": 7470294,
            "revision": "3bd368ab0f3da472b1adc6e19d37717a6cd0967f",
            "sha256": "74b0686a3d2891980d5e303251b075a3bccae2c2ff650747db2620a649b98fa8",
            "check": "pass"
        },
        {
            "name": "model-00001-of-00048.safetensors",
            "size": 970533624,
            "revision": "9e55e683e23a488ca81668102b4cae8473995f5c",
            "sha256": "886aebdafa08cc27bbae2165ed35bdfe0de9370bf88c1411283c155c6ae4ff89",
            "check": "pass"
        }
    ]
}
```

## files

### check

pending: 上游哈希已获取,本地尚未校验
pass: 本地 SHA-256 与上游一致
fail: 本地 SHA-256 或大小与上游不一致

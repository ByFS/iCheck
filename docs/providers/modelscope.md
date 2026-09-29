---
created: 2026-09-25T00:40:00+08:00
updated: 2026-09-29T11:36:00+08:00
---

## 接口请求

### 接口一

GET `https://www.modelscope.cn/api/v1/models/{author}/{model_name}/repo/files?Revision={revision}&Recursive=true`

E.g <https://www.modelscope.cn/api/v1/models/deepseek-ai/DeepSeek-V4.1-Flash/repo/files?Revision=master&Recursive=true>

`Revision` 传分支名或完整 commit SHA 均可
`Recursive=true` 必带,否则只返回顶层

| 字段                  | 演示值                      | 对应 official_hash | 说明                        |
| --------------------- | --------------------------- | ---------------------- | --------------------------- |
| `Data.Files.Path`     | `assets/dsv41_kv_cache.png` | `files[].name`         | 仓库相对路径,含子目录       |
| `Data.Files.Size`     | `270933`                    | `files[].size`         | 上游报的内容字节数          |
| `Data.Files.Sha256`   | `b61bf465...793e4b`         | `files[].sha256`       | 内容 SHA-256(64 hex)        |
| `Data.Files.Revision` | `fe66e56d...077aaf`         | `files[].revision`     | 该文件最后一次修改的 commit |
| `Data.Files.Type`     | `blob`                      | 不落盘                 | `blob`=文件,`tree`=目录     |

### 接口二

GET `https://modelscope.cn/api/v1/models/{author}/{model_name}`

E.g <https://modelscope.cn/api/v1/models/deepseek-ai/DeepSeek-V4.1-Flash>

| 字段                                    | 演示值                       | 对应 official_hash |
| --------------------------------------- | ---------------------------- | ------------------ |
| Data.ModelInfos.safetensor.files.name   | model.safetensors.index.json | file.name          |
| Data.ModelInfos.safetensor.files.Size   | 7470294                      | file.size          |
| Data.ModelInfos.safetensor.files.Sha256 | 74b0686a...b98fa8            | file.sha256        |

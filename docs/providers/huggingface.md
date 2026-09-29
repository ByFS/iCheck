---
created: 2026-09-25T00:40:00+08:00
updated: 2026-09-29T10:53:00+08:00
---

## 接口请求

### 接口一

优先使用一号接口格式

GET `https://huggingface.co/api/models/{author}/{model_name}?blobs=true`

E.g <https://huggingface.co/api/models/deepseek-ai/DeepSeek-V4.1-Flash?blobs=true>

| 字段                | 演示值                           | 对应 official_hash |
| ------------------- | -------------------------------- | ------------------ |
| modelId             | deepseek-ai/DeepSeek-V4.1-Flash  | source.model_id    |
| siblings.rfilename  | model-00001-of-00048.safetensors | file.name          |
| siblings.lfs.size   | 970533624                        | file.size          |
| siblings.lfs.sha256 | 886aebda...e4ff89                | file.sha256        |

### 接口二

次要使用二号接口格式

`https://huggingface.co/{author}/{model_name}/raw/main/{file_name}`

示例

<https://huggingface.co/deepseek-ai/DeepSeek-V4.1-Flash/raw/main/model-00001-of-00048.safetensors>

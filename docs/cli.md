---
created: 2026-09-24T23:58:00+08:00
updated: 2026-09-28T17:50:00+08:00
---

## 基本用法

```bash
./icheck <command> <path>
```

command

| 参数      | 介绍                         |
| ---------- | ---------------------------- |
| `check`    | 获取官方 Hash 并校验本地模型 |
| `generate` | 根据本地模型生成锚点 Hash    |
| `verify`   | 根据锚点 Hash 校验本地模型   |

### check

`check` 命令在模型目录中生成官方 Hash 并校验模型

```bash
./icheck check <path> <source> <author>/<model>
```

source

| 参数 | 来源        | 介绍                                            |
| ---- | ----------- | ----------------------------------------------- |
| ms   | ModelScope  | [ModelScope 设计文档](providers/modelscope.md)  |
| hf   | HuggingFace | [HuggingFace 设计文档](providers/huggingface.md) |

例如当前目录结构

```txt
deepseek-ai/
└── DeepSeek-V4.1-Flash/
    ├── model.safetensors.index.json
    ├── model-00001-of-00048.safetensors
    ├── model-00002-of-00048.safetensors
    └── ...
```

执行

```bash    
# 检查模型的 ModelScope 官方 Hash
./icheck check /data/models/deepseek-ai/DeepSeek-V4.1-Flash ms deepseek-ai/DeepSeek-V4.1-Flash
INFO: Source: ModelScope
INFO: Model_id: deepseek-ai/DeepSeek-V4.1-Flash
INFO: Obtain model information
INFO: Files: 79
INFO: Start obtaining information

INFO: name: model.safetensors.index.json
INFO: sha256: 74b0686a3d2891980d5e303251b075a3bccae2c2ff650747db2620a649b98fa8
INFO: size: 7470294
INFO: name: model-00001-of-00048.safetensors
INFO: size: 970533624
INFO: sha256: 886aebdafa08cc27bbae2165ed35bdfe0de9370bf88c1411283c155c6ae4ff89
INFO: name: model-00002-of-00048.safetensors
INFO: size: 1323858272
INFO: sha256: 4320066fc6958e5bc01d8c3feba79b7454b59f0f4b7299ab7145ed44bbf4ecec
...
INFO: name: model-00048-of-00048.safetensors
INFO: size: 101537926640
INFO: sha256: 976330f4954338e1ad8b508c32aa912032c7ad908959fd53c8307650fe4520ed

INFO: Checking model

[OK] model.safetensors.index.json
[OK] model-00001-of-00048.safetensors
[OK] model-00002-of-00048.safetensors
...
[OK] model-00048-of-00048.safetensors

Files: 89
Passed: 80
Failed: 0

Result: PASS
```

huggingface 的同理

```bash
# 检查模型的 HuggingFace 官方 Hash
./icheck check /data/models/deepseek-ai/DeepSeek-V4.1-Flash hf deepseek-ai/DeepSeek-V4.1-Flash
INFO: Source: HuggingFace
INFO: Model_id: deepseek-ai/DeepSeek-V4.1-Flash
INFO: Obtain model information
INFO: Files: 89
INFO: Start obtaining information

INFO: name: model.safetensors.index.json
INFO: sha256: 74b0686a3d2891980d5e303251b075a3bccae2c2ff650747db2620a649b98fa8
INFO: size: 7470294
INFO: name: model-00001-of-00048.safetensors
INFO: size: 970533624
INFO: sha256: 886aebdafa08cc27bbae2165ed35bdfe0de9370bf88c1411283c155c6ae4ff89
INFO: name: model-00002-of-00048.safetensors
INFO: size: 1323858272
INFO: sha256: 4320066fc6958e5bc01d8c3feba79b7454b59f0f4b7299ab7145ed44bbf4ecec
...
INFO: name: model-00048-of-00048.safetensors
INFO: size: 101537926640
INFO: sha256: 976330f4954338e1ad8b508c32aa912032c7ad908959fd53c8307650fe4520ed

INFO: Checking model

[OK] model.safetensors.index.json
[OK] model-00001-of-00048.safetensors
[OK] model-00002-of-00048.safetensors
...
[OK] model-00048-of-00048.safetensors
INFO: Result: PASS
```

会生成 official_hash.json

```txt
deepseek-ai/
└── DeepSeek-V4.1-Flash/
    ├── model.safetensors.index.json
    ├── model-00001-of-00048.safetensors
    ├── ...
    └── .iCheck
        └── official
            └── official_hash.json
```

设计细节见 [official_hash](specs/official_hash.md)

### generate

假设当前目录结构

```txt
deepseek-ai/
└── DeepSeek-V4.1-Flash/
    ├── model.safetensors.index.json
    ├── model-00001-of-00048.safetensors
    └── ...
```

执行 `generate` 命令会在模型目录中生成 [锚点索引](specs/anchor_index.md) 和 [锚点哈希](specs/anchor_hash.md)

```bash
./icheck generate /data/models/deepseek-ai/DeepSeek-V4.1-Flash
INFO: name: model.safetensors.index.json
INFO: BLAKE3: xxx
INFO: size: 7470294
INFO: name: model-00001-of-00048.safetensors
INFO: size: 970533624
INFO: BLAKE3: xxx
INFO: name: model-00002-of-00048.safetensors
INFO: size: 1323858272
INFO: BLAKE3: xxx
...
INFO: name: model-00048-of-00048.safetensors
INFO: size: 101537926640
INFO: BLAKE3: xxx
```

生成

```txt
deepseek-ai/
└── DeepSeek-V4.1-Flash/
    ├── model.safetensors.index.json
    ├── model-00001-of-00048.safetensors
    ├── ...
    └── .iCheck
        └── anchor
            ├── anchor_index.json
            └── anchor_hash.json
```

[锚点索引](specs/anchor_index.md) 设计细节
[锚点哈希](specs/anchor_hash.md) 设计细节

### verify

使用已经生成的 [锚点索引](specs/anchor_index.md) 校验模型

```bash
./icheck verify /data/models/deepseek-ai/DeepSeek-V4.1-Flash
INFO: Start quick check
INFO: Model_id: deepseek-ai/DeepSeek-V4.1-Flash
INFO: Load anchor index
INFO: Files: 89
INFO: Quick check passed
INFO: Start verification
[OK] model.safetensors.index.json
[OK] model-00001-of-00048.safetensors
[OK] model-00002-of-00048.safetensors
...
[OK] model-00048-of-00048.safetensors

Files: 89
Passed: 89
Failed: 0

Result: PASS
```

## 失败样例

```bash
./icheck verify /data/models/deepseek-ai/DeepSeek-V4.1-Flash
INFO: Start quick check
INFO: Model_id: deepseek-ai/DeepSeek-V4.1-Flash
INFO: Load anchor index
INFO: Files: 89

[MISSING]       model-00042-of-00048.safetensors
[SIZE-MISMATCH] model-00013-of-00048.safetensors
                expected: 7389761368
                actual:   4194304000

INFO: Quick check found 2 problem(s), start verification

[OK]   config.json
[OK]   model-00001-of-00048.safetensors
...
[FAIL] model-00007-of-00048.safetensors
       size:     7389759032 (与锚点一致)
       expected: f03ee0b7a016781235e0c135f16710f54d2492d3f2a1ea0a6f7bad857fc41c4f
       actual:   6b1d9e4a72c0f38b5e8d1a4f7c2b9e6d3a8f5c1e4b7d0a3f6c9e2b5d8a1f4c7e

Files: 89
Passed: 86
Failed: 3

Result: FAIL
```

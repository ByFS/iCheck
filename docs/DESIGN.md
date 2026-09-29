---
created: 2026-09-24T10:54:00+08:00
updated: 2026-09-28T18:13:00+08:00
---

## 术语

| 术语     | 设计文档                                | 含义                           |
| -------- | --------------------------------------- | ------------------------------ |
| 官方哈希 | [official_hash](specs/official_hash.md) | 来自下载平台的一些元数据和哈希 |
| 锚点索引 | [anchor_index](specs/anchor_index.md)   | 包含锚点配置信息的元数据       |
| 锚点哈希 | [anchor_hash](specs/anchor_hash.md)     | 用于本程序校验的主哈希         |

## 原理

iCheck 的信任基础是一条锚点链

1. 首次校验以平台提供的哈希为基准
2. 首次校验通过后对文件内容计算 BLAKE3 写入本地锚点索引
3. 后续分发检查时重新计算 BLAKE3 并与锚点比对
4. 若锚点不一致则进一步排查文件是否分发过程出现损坏

> [!IMPORTANT] 信任边界
> 锚点仅用于证明 "文件与首次记录一致" 不能替代官方校验

## 性能

典型 NVMe SSD 和多线程 CPU 的环境中, 锚点校验相比每次重新计算 SHA-256 最高可提升约 30 倍

在 HDD 与 SATA SSD 上读带宽先成为瓶颈, BLAKE3 相对 SHA-256 的优势有限, 只有在高性能场景下, 多线程 BLAKE3 才能兑现全部收益

## 流程

```txt
                 Internet
                    │
                    ▼
               用户下载模型
          User downloads the models
                    │
                    ▼
         ┌──────────────────────┐
         │     iCheck check     │
         │                      │
         │   获取官方校验信息   │
         │   Obtain official    │
         │  verification data   │
         └──────────┬───────────┘
                    │
                 验证通过
                    │
                    ▼
         ┌──────────────────────┐
         │   iCheck generate    │
         │                      │
         │    生成可信锚点      │
         │ Trusted Anchor Points│
         └──────────┬───────────┘
                    │
                    ▼
      ╔═══════════════════════════╗
      ║        内网模型分发       ║
      ║   Internal Distribution   ║
      ╚═════════════╤═════════════╝
                    │
         ┌──────────┴──────────┐
         │                     │
         ▼                     ▼
     内网机器 A              内网机器 B
Internal Machine A      Internal Machine B
         │                     │
         ▼                     ▼
   iCheck verify          iCheck verify

   检查可信锚点           检查可信锚点
Verify Anchor Points   Verify Anchor Points
         │                     │
         └──────────┬──────────┘
                    ▼
                   比较
                 Compare
                    │
             ┌──────┴──────┐
             │             │
             ▼             ▼
           通过           失败
           PASS           FAIL
```

### check

建立官方基准 (需要联网)

产出 `.iCheck/official/official_hash.json`

1. 请求上游文件清单, 清洗数据写入 `official_hash.json`
2. 遍历本地模型目录, 逐文件计算 SHA-256
3. 与上游清单逐条比对, 先比大小, 大小不符的直接记为失败不再计算哈希
4. 通过校验后写 `official_hash.json` 的 `check` 字段从 `pending` 改为 `pass`
5. 已有 `official_hash.json` 且与本批结果不一致时不覆盖, 报告差异后退出

### generate

建立本地锚点(离线)

产出 `.iCheck/anchor/anchor_hash.json` 和 `.iCheck/anchor/anchor_index.json`

1. 读取 `official_hash.json` 取模型身份信息
2. 遍历本地模型目录, 逐文件计算 BLAKE3
3. 写 `anchor_hash.json`
4. 汇总覆盖情况与逐文件大小, 最后写 `anchor_index.json`

### verify

校验本地模型(离线)

前提 `anchor_index.json` 与 `anchor_hash.json` 已存在

1. 读 `anchor_index.json` 取排除规则与两份数据文件位置
2. 从 `anchor_hash.json` 取逐文件清单与大小做集合级比对
3. 集合级比对
    - MISSING 索引有记录但磁盘无文件
    - SIZE-MISMATCH 文件大小不匹配
    - ADDED 磁盘有文件单索引无记录
4. 只对集合级对比通过的文件计算 BLAKE3
5. 与 `anchor_hash.json` 的基准值比对
6. 分开输出 "集合级差异" 和 "内容不符" 的结果与汇总

## 功能

参见 [cli](cli.md)

## 上游

[modelscope 设计文档](providers/modelscope.md)

[huggingface 设计文档](providers/huggingface.md)

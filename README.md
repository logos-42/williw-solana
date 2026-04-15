# Solana 算力共享与 API 消耗追踪智能合约

这是一个基于 Solana 区块链的智能合约项目，用于管理算力共享服务和 API 消耗追踪。用户可以通过手机钱包订阅服务计划，也可以贡献自己的手机算力来获得收益。

## 功能特性

### 1. 用户功能
- **钱包绑定**：初始化用户账户并绑定 Solana 钱包
- **订阅计划**：支付 SOL 订阅不同等级的服务计划
  - Basic（基础版）：1 SOL / 月，10,000 API 额度
  - Pro（专业版）：5 SOL / 月，100,000 API 额度
  - Enterprise（企业版）：20 SOL / 月，500,000 API 额度
- **API 消耗追踪**：自动扣除 API 调用额度

### 2. 算力提供者功能
- **注册为提供者**：将设备注册为算力提供节点
- **贡献算力**：提交算力工作并获得收益
- **收益结算**：按照智能合约自动计算收益（每 1000 计算单位 = 0.001 SOL）
- **提现收益**：随时提取累计的收益到钱包

### 3. 平台管理
- **收入统计**：追踪平台总收入
- **算力统计**：记录总算力贡献
- **API 调用统计**：监控 API 使用情况

## 技术架构

- **区块链**：Solana
- **开发框架**：Anchor Framework 0.29.0
- **编程语言**：Rust (智能合约) + TypeScript (测试)
- **账户模型**：使用 PDA (Program Derived Address) 确保安全性

## 项目结构

```
.
├── programs/
│   └── compute-power/
│       ├── src/
│       │   └── lib.rs          # 智能合约主代码
│       ├── Cargo.toml
│       └── Xargo.toml
├── tests/
│   └── compute-power.ts        # 测试用例
├── Anchor.toml                 # Anchor 配置
├── Cargo.toml                  # Rust 工作空间配置
├── package.json                # Node.js 依赖
└── README.md                   # 项目文档
```

## 安装与部署

### 前置要求

1. 安装 Rust：
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

2. 安装 Solana CLI：
```bash
sh -c "$(curl -sSfL https://release.solana.com/stable/install)"
```

3. 安装 Anchor：
```bash
cargo install --git https://github.com/coral-xyz/anchor avm --locked --force
avm install latest
avm use latest
```

4. 安装 Node.js 依赖：
```bash
npm install
# 或
yarn install
```

### 构建项目

```bash
anchor build
```

### 运行测试

```bash
anchor test
```

### 部署到 Devnet

1. 配置 Solana CLI 到 devnet：
```bash
solana config set --url devnet
```

2. 创建钱包（如果还没有）：
```bash
solana-keygen new
```

3. 获取测试 SOL：
```bash
solana airdrop 2
```

4. 部署合约：
```bash
anchor deploy
```

## 智能合约接口

### 1. `initialize_user()`
初始化用户账户并绑定钱包。

### 2. `subscribe_plan(plan: SubscriptionPlan)`
订阅服务计划并支付费用。
- 参数：`Basic` | `Pro` | `Enterprise`

### 3. `register_as_provider()`
将当前用户注册为算力提供者。

### 4. `submit_compute_work(compute_units: u64)`
提交算力工作（由验证节点调用）。
- 参数：计算单位数量

### 5. `withdraw_earnings()`
提现累计的算力收益。

### 6. `consume_api_credits(credits: u64)`
消耗 API 调用额度。
- 参数：要消耗的额度数量

### 7. `initialize_platform()`
初始化平台账户（仅管理员）。

## 手机端集成指南

### 使用 Solana Mobile SDK

1. 安装依赖：
```bash
npm install @solana-mobile/mobile-wallet-adapter-protocol
npm install @solana/web3.js
```

2. 连接钱包示例（React Native）：
```typescript
import { transact } from '@solana-mobile/mobile-wallet-adapter-protocol';
import { Connection, PublicKey } from '@solana/web3.js';

// 连接钱包
const connectWallet = async () => {
  const result = await transact(async (wallet) => {
    const authorization = await wallet.authorize({
      cluster: 'devnet',
      identity: { name: 'Compute Power App' },
    });
    return authorization;
  });
  return result;
};
```

3. 订阅计划示例：
```typescript
import * as anchor from '@coral-xyz/anchor';

const subscribeToPlan = async (wallet, plan) => {
  const connection = new Connection('https://api.devnet.solana.com');
  const provider = new anchor.AnchorProvider(connection, wallet, {});
  const program = new anchor.Program(IDL, PROGRAM_ID, provider);
  
  await program.methods
    .subscribePlan(plan)
    .accounts({
      userAccount: userAccountPda,
      platformAccount: platformAccountPda,
      user: wallet.publicKey,
      systemProgram: SystemProgram.programId,
    })
    .rpc();
};
```

## 收益计算模型

算力提供者的收益按以下公式计算：

```
收益 (SOL) = (贡献的计算单位 / 1000) × 0.001
```

例如：
- 贡献 5,000 计算单位 = 0.005 SOL
- 贡献 100,000 计算单位 = 0.1 SOL

## 安全考虑

1. **PDA 账户**：使用 Program Derived Address 确保账户安全
2. **权限验证**：所有操作都需要用户签名
3. **订阅验证**：API 调用前检查订阅是否有效
4. **溢出保护**：所有数值计算都有溢出检查

## 开发路线图

- [x] 基础智能合约开发
- [x] 订阅计划系统
- [x] 算力贡献与结算
- [ ] 手机端 SDK 开发
- [ ] 算力验证机制优化
- [ ] 多级收益分配系统
- [ ] 治理代币集成

## 许可证

MIT License

## 联系方式

如有问题或建议，欢迎提交 Issue 或 Pull Request。

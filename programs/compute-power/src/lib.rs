use anchor_lang::prelude::*;

declare_id!("7ng11Nyy312EbcsKmEnugTfBnkCLpiqxVLMCwBSUVK3a");

// ============ 常量 ============

/// 最小提现金额: 0.001 SOL
const MIN_WITHDRAW_AMOUNT: u64 = 1_000_000;
/// 提现冷却时间: 1小时
const WITHDRAW_COOLDOWN: i64 = 3600;
/// 单次最大提交 token 数: 1000万
const MAX_TOKENS_PER_SUBMIT: u64 = 10_000_000;
/// 单次最小提交 token 数: 防止垃圾交易攻击
const MIN_TOKENS_PER_SUBMIT: u64 = 100;
/// 最大汇率变化百分比: 50% (防止汇率操纵攻击)
const MAX_RATE_CHANGE_PERCENT: u64 = 50;
/// 汇率更新最小间隔: 5分钟 (防止频繁更新攻击)
const MIN_RATE_UPDATE_INTERVAL: i64 = 300;
/// 平台账户最小余额: 0.1 SOL (防止耗尽攻击)
const MIN_PLATFORM_BALANCE: u64 = 100_000_000;

/// 抽佣比例
const DEV_FEE_BPS: u64 = 3000;    // 30% = 3000 基点
const PROVIDER_FEE_BPS: u64 = 7000; // 70% = 7000 基点
const BPS_BASE: u64 = 10_000;     // 100% = 10000 基点

// 算力成本 (单位: 元 / 百万 tokens, 人民币计价)
// 百万 tokens 输入 (缓存命中): 0.5 元
// 百万 tokens 输入 (缓存未命中): 3 元
// 百万 tokens 输出: 5 元
const COST_CACHE_HIT_YUAN_PER_MILLION: u64 = 500;       // 0.5 元 = 500 分
const COST_CACHE_MISS_YUAN_PER_MILLION: u64 = 3_000;    // 3 元 = 3000 分
const COST_OUTPUT_YUAN_PER_MILLION: u64 = 5_000;        // 5 元 = 5000 分
const YUAN_FEN_BASE: u64 = 1_000; // 1 元 = 1000 分 (3位小数精度)

/// 默认汇率: 1 CNY = X lamports
/// 2026-04-17: 1 SOL = $88.55, $1 = ¥6.8622 → 1 SOL = ¥607.75 → 1 CNY = 1,645,483 lamports
const DEFAULT_LAMPORTS_PER_YUAN: u64 = 1_645_483;

/// Pyth SOL/USD 价格源 (mainnet)
const PYTH_SOL_USD_FEED: &str = "H6ARHf6YXhGYeQfUzQNGk6rpnnaa7mpZ1Cp9ekWf66Dw";
/// Pyth SOL/USD 价格源 (devnet)
const PYTH_SOL_USD_FEED_DEVNET: &str = "J83w4HKfqxwcq3bEMyk13V9YLG1WnAUuUsQ3bch73cwr";

#[program]
pub mod compute_power {
    use super::*;

    /// 初始化用户账户
    pub fn initialize_user(ctx: Context<InitializeUser>) -> Result<()> {
        let user_account = &mut ctx.accounts.user_account;
        user_account.owner = ctx.accounts.user.key();
        user_account.total_tokens_consumed = 0;
        user_account.total_spent = 0;
        user_account.is_provider = false;
        user_account.compute_power_contributed = 0;
        user_account.pending_revenue = 0;
        user_account.withdrawn = 0;
        user_account.last_withdraw_time = 0;
        user_account.bump = ctx.bumps.user_account;

        msg!("用户账户初始化成功: {}", ctx.accounts.user.key());
        Ok(())
    }

    /// 注册成为算力节点提供者
    pub fn register_as_provider(ctx: Context<RegisterProvider>) -> Result<()> {
        let user_account = &mut ctx.accounts.user_account;

        require!(
            user_account.owner == ctx.accounts.user.key(),
            ErrorCode::Unauthorized
        );

        require!(!user_account.is_provider, ErrorCode::AlreadyProvider);

        user_account.is_provider = true;
        user_account.compute_power_contributed = 0;
        user_account.pending_revenue = 0;
        user_account.withdrawn = 0;
        user_account.last_withdraw_time = 0;

        msg!("用户 {} 注册为算力节点提供者", ctx.accounts.user.key());
        Ok(())
    }

    /// 提交算力工作（用户为 API 调用付费，费用自动分成）
    /// 调用者(user)为 API 使用者，provider_account 为算力提供者
    pub fn submit_compute_work(
        ctx: Context<SubmitComputeWork>,
        input_tokens_cache_hit: u64,
        input_tokens_cache_miss: u64,
        output_tokens: u64,
    ) -> Result<()> {
        let provider_account = &mut ctx.accounts.provider_account;
        let platform_account = &mut ctx.accounts.platform_account;

        require!(provider_account.is_provider, ErrorCode::NotProvider);

        // 防止用户和提供者是同一人（自我交易攻击）
        require!(
            ctx.accounts.user.key() != ctx.accounts.provider.key(),
            ErrorCode::SelfTransactionNotAllowed
        );

        let total_tokens = input_tokens_cache_hit
            .checked_add(input_tokens_cache_miss)
            .ok_or(ErrorCode::CalculationOverflow)?
            .checked_add(output_tokens)
            .ok_or(ErrorCode::CalculationOverflow)?;

        // 防止垃圾交易和过大交易攻击
        require!(
            total_tokens >= MIN_TOKENS_PER_SUBMIT && total_tokens <= MAX_TOKENS_PER_SUBMIT,
            ErrorCode::InvalidTokenAmount
        );

        // 从平台账户读取当前汇率
        let rate = platform_account.lamports_per_yuan;

        // 计算三种 token 的成本 (人民币分 → lamports)
        // 公式: (tokens * 分/百万tokens) / 1_000_000 * lamports/分
        let cost_cache_hit = calc_cost(input_tokens_cache_hit, COST_CACHE_HIT_YUAN_PER_MILLION, rate)?;
        let cost_cache_miss = calc_cost(input_tokens_cache_miss, COST_CACHE_MISS_YUAN_PER_MILLION, rate)?;
        let cost_output = calc_cost(output_tokens, COST_OUTPUT_YUAN_PER_MILLION, rate)?;

        let total_cost = cost_cache_hit
            .checked_add(cost_cache_miss)
            .ok_or(ErrorCode::CalculationOverflow)?
            .checked_add(cost_output)
            .ok_or(ErrorCode::CalculationOverflow)?;

        // 防止零成本攻击
        require!(total_cost > 0, ErrorCode::InvalidCostAmount);

        // 检查用户余额是否足够（防止余额不足攻击）
        let user_balance = ctx.accounts.user.to_account_info().lamports();
        require!(
            user_balance >= total_cost.checked_add(5_000).ok_or(ErrorCode::CalculationOverflow)?,
            ErrorCode::InsufficientUserBalance
        );

        // 按基点计算分成
        let dev_fee = (total_cost as u128)
            .checked_mul(DEV_FEE_BPS as u128)
            .ok_or(ErrorCode::CalculationOverflow)?
            .checked_div(BPS_BASE as u128)
            .ok_or(ErrorCode::CalculationOverflow)? as u64;

        let provider_fee = (total_cost as u128)
            .checked_mul(PROVIDER_FEE_BPS as u128)
            .ok_or(ErrorCode::CalculationOverflow)?
            .checked_div(BPS_BASE as u128)
            .ok_or(ErrorCode::CalculationOverflow)? as u64;

        // 验证分成总和不超过总成本（防止计算错误）
        require!(
            dev_fee.checked_add(provider_fee).ok_or(ErrorCode::CalculationOverflow)? <= total_cost,
            ErrorCode::InvalidFeeCalculation
        );

        // 第一步：用户 → 平台账户 (全额)
        **ctx.accounts.user.to_account_info().try_borrow_mut_lamports()? = ctx
            .accounts
            .user
            .to_account_info()
            .lamports()
            .checked_sub(total_cost)
            .ok_or(ErrorCode::CalculationOverflow)?;

        **platform_account.to_account_info().try_borrow_mut_lamports()? = platform_account
            .to_account_info()
            .lamports()
            .checked_add(total_cost)
            .ok_or(ErrorCode::CalculationOverflow)?;

        // 第二步：平台账户 → 开发者 (30% 抽佣)
        **platform_account.to_account_info().try_borrow_mut_lamports()? = platform_account
            .to_account_info()
            .lamports()
            .checked_sub(dev_fee)
            .ok_or(ErrorCode::CalculationOverflow)?;

        **ctx.accounts.dev_wallet.to_account_info().try_borrow_mut_lamports()? = ctx
            .accounts
            .dev_wallet
            .to_account_info()
            .lamports()
            .checked_add(dev_fee)
            .ok_or(ErrorCode::CalculationOverflow)?;

        // 更新算力提供者收益 (70%)
        provider_account.compute_power_contributed = provider_account
            .compute_power_contributed
            .checked_add(total_tokens)
            .ok_or(ErrorCode::CalculationOverflow)?;

        provider_account.pending_revenue = provider_account
            .pending_revenue
            .checked_add(provider_fee)
            .ok_or(ErrorCode::CalculationOverflow)?;

        // 更新平台统计
        platform_account.total_revenue = platform_account
            .total_revenue
            .checked_add(total_cost)
            .ok_or(ErrorCode::CalculationOverflow)?;

        platform_account.total_dev_fees = platform_account
            .total_dev_fees
            .checked_add(dev_fee)
            .ok_or(ErrorCode::CalculationOverflow)?;

        platform_account.total_provider_fees = platform_account
            .total_provider_fees
            .checked_add(provider_fee)
            .ok_or(ErrorCode::CalculationOverflow)?;

        platform_account.total_compute_units = platform_account
            .total_compute_units
            .checked_add(total_tokens)
            .ok_or(ErrorCode::CalculationOverflow)?;

        platform_account.total_api_calls = platform_account
            .total_api_calls
            .checked_add(1)
            .ok_or(ErrorCode::CalculationOverflow)?;

        // 更新用户消费记录
        let user_account = &mut ctx.accounts.user_account;
        user_account.total_tokens_consumed = user_account
            .total_tokens_consumed
            .checked_add(total_tokens)
            .ok_or(ErrorCode::CalculationOverflow)?;

        user_account.total_spent = user_account
            .total_spent
            .checked_add(total_cost)
            .ok_or(ErrorCode::CalculationOverflow)?;

        msg!(
            "算力消费: cache_hit={} cache_miss={} output={} | 汇率={} lam/元 | 费用={} lam dev={} prov={}",
            input_tokens_cache_hit, input_tokens_cache_miss, output_tokens,
            rate, total_cost, dev_fee, provider_fee
        );
        Ok(())
    }

    /// 算力提供者提现收益
    pub fn withdraw_earnings(ctx: Context<WithdrawEarnings>) -> Result<()> {
        let provider_account = &mut ctx.accounts.provider_account;
        let platform_account = &mut ctx.accounts.platform_account;

        require!(
            provider_account.owner == ctx.accounts.user.key(),
            ErrorCode::Unauthorized
        );

        require!(provider_account.is_provider, ErrorCode::NotProvider);
        require!(provider_account.pending_revenue > 0, ErrorCode::NoEarnings);

        require!(
            provider_account.pending_revenue >= MIN_WITHDRAW_AMOUNT,
            ErrorCode::BelowMinimumWithdraw
        );

        let clock = Clock::get()?;
        require!(
            clock.unix_timestamp >= provider_account.last_withdraw_time + WITHDRAW_COOLDOWN,
            ErrorCode::WithdrawCooldown
        );

        let earnings = provider_account.pending_revenue;

        let platform_balance = platform_account.to_account_info().lamports();
        
        // 确保平台账户有足够余额，且保留最小余额（防止耗尽攻击）
        require!(
            platform_balance >= earnings.checked_add(MIN_PLATFORM_BALANCE).ok_or(ErrorCode::CalculationOverflow)?,
            ErrorCode::InsufficientPlatformBalance
        );

        // 先更新状态，再转账（CEI 模式，防止重入攻击）
        provider_account.pending_revenue = 0;
        provider_account.withdrawn = provider_account
            .withdrawn
            .checked_add(earnings)
            .ok_or(ErrorCode::CalculationOverflow)?;
        provider_account.last_withdraw_time = clock.unix_timestamp;

        // 使用直接 lamports 操作而非 CPI（更安全，防止 CPI 攻击）
        **platform_account.to_account_info().try_borrow_mut_lamports()? = platform_account
            .to_account_info()
            .lamports()
            .checked_sub(earnings)
            .ok_or(ErrorCode::CalculationOverflow)?;

        **ctx.accounts.user.to_account_info().try_borrow_mut_lamports()? = ctx
            .accounts
            .user
            .to_account_info()
            .lamports()
            .checked_add(earnings)
            .ok_or(ErrorCode::CalculationOverflow)?;

        msg!("提现成功: {} lamports", earnings);
        Ok(())
    }

    /// 初始化平台账户
    pub fn initialize_platform(ctx: Context<InitializePlatform>) -> Result<()> {
        let platform_account = &mut ctx.accounts.platform_account;
        platform_account.authority = ctx.accounts.authority.key();
        platform_account.dev_wallet = ctx.accounts.dev_wallet.key();
        platform_account.lamports_per_yuan = DEFAULT_LAMPORTS_PER_YUAN;
        platform_account.last_rate_update = Clock::get()?.unix_timestamp;
        platform_account.total_revenue = 0;
        platform_account.total_dev_fees = 0;
        platform_account.total_provider_fees = 0;
        platform_account.total_compute_units = 0;
        platform_account.total_api_calls = 0;
        platform_account.bump = ctx.bumps.platform_account;

        msg!(
            "平台账户初始化成功, 开发者钱包: {}, 默认汇率: {} lam/元",
            ctx.accounts.dev_wallet.key(),
            DEFAULT_LAMPORTS_PER_YUAN
        );
        Ok(())
    }

    /// 更新汇率 (仅管理员)
    /// lamports_per_yuan: 1 元人民币对应的 lamports 数量
    /// 计算方式: lamports_per_yuan = 1_000_000_000 / (SOL_USD * USD_CNY)
    /// 例: SOL=$150, 1$=7.2元 → 1SOL=1080元 → 1元=925,926 lamports
    pub fn update_exchange_rate(
        ctx: Context<UpdateExchangeRate>,
        lamports_per_yuan: u64,
    ) -> Result<()> {
        require!(
            lamports_per_yuan >= 100_000 && lamports_per_yuan <= 100_000_000,
            ErrorCode::InvalidExchangeRate
        );

        let platform_account = &mut ctx.accounts.platform_account;
        let clock = Clock::get()?;
        
        // 防止频繁更新攻击
        require!(
            clock.unix_timestamp >= platform_account.last_rate_update + MIN_RATE_UPDATE_INTERVAL,
            ErrorCode::RateUpdateTooFrequent
        );

        let old_rate = platform_account.lamports_per_yuan;
        
        // 防止汇率剧烈变化攻击（最多变化 50%）
        let max_rate = old_rate
            .checked_mul(100 + MAX_RATE_CHANGE_PERCENT)
            .ok_or(ErrorCode::CalculationOverflow)?
            .checked_div(100)
            .ok_or(ErrorCode::CalculationOverflow)?;
        
        let min_rate = old_rate
            .checked_mul(100 - MAX_RATE_CHANGE_PERCENT)
            .ok_or(ErrorCode::CalculationOverflow)?
            .checked_div(100)
            .ok_or(ErrorCode::CalculationOverflow)?;
        
        require!(
            lamports_per_yuan >= min_rate && lamports_per_yuan <= max_rate,
            ErrorCode::RateChangeTooLarge
        );

        platform_account.lamports_per_yuan = lamports_per_yuan;
        platform_account.last_rate_update = clock.unix_timestamp;

        msg!(
            "汇率更新: {} → {} lam/元 (1 SOL ≈ {} CNY)",
            old_rate,
            lamports_per_yuan,
            1_000_000_000 / lamports_per_yuan
        );
        Ok(())
    }
}

/// 计算费用: tokens * (分/百万tokens) / 1_000_000 * (lamports/分)
/// 等价于: tokens * 分/百万tokens * lamports_per_yuan / 1_000_000 / 1_000
fn calc_cost(tokens: u64, yuan_fen_per_million: u64, lamports_per_yuan: u64) -> Result<u64> {
    // 防止零值攻击
    if tokens == 0 {
        return Ok(0);
    }

    // 3步计算避免 u128 溢出，同时保持精度
    // tokens * yuan_fen_per_million → 最多 10M * 5000 = 50G，u64 够用
    let yuan_fen_total = (tokens as u128)
        .checked_mul(yuan_fen_per_million as u128)
        .ok_or(ErrorCode::CalculationOverflow)?;

    // / 1_000_000 (百万 tokens)
    let yuan_fen = yuan_fen_total
        .checked_div(1_000_000)
        .ok_or(ErrorCode::CalculationOverflow)?;

    // * lamports_per_yuan (最多 ~1M)
    let lamports_128 = yuan_fen
        .checked_mul(lamports_per_yuan as u128)
        .ok_or(ErrorCode::CalculationOverflow)?;

    // 确保结果在 u64 范围内
    let lamports = lamports_128
        .try_into()
        .map_err(|_| ErrorCode::CalculationOverflow)?;

    Ok(lamports)
}

// ============ 账户结构 ============

#[derive(Accounts)]
pub struct InitializeUser<'info> {
    #[account(
        init,
        payer = user,
        space = 8 + UserAccount::INIT_SPACE,
        seeds = [b"user", user.key().as_ref()],
        bump
    )]
    pub user_account: Account<'info, UserAccount>,

    #[account(mut)]
    pub user: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct RegisterProvider<'info> {
    #[account(
        mut,
        seeds = [b"user", user.key().as_ref()],
        bump = user_account.bump,
        constraint = user_account.owner == user.key() @ ErrorCode::Unauthorized
    )]
    pub user_account: Account<'info, UserAccount>,

    #[account(mut)]
    pub user: Signer<'info>,
}

#[derive(Accounts)]
pub struct SubmitComputeWork<'info> {
    /// API 调用者（付费用户）
    #[account(mut)]
    pub user: Signer<'info>,

    /// 付费用户的账户记录
    #[account(
        mut,
        seeds = [b"user", user.key().as_ref()],
        bump = user_account.bump,
        constraint = user_account.owner == user.key() @ ErrorCode::Unauthorized
    )]
    pub user_account: Account<'info, UserAccount>,

    /// 算力提供者的账户
    #[account(
        mut,
        seeds = [b"user", provider.key().as_ref()],
        bump = provider_account.bump,
        constraint = provider_account.owner == provider.key() @ ErrorCode::Unauthorized,
        constraint = provider_account.is_provider @ ErrorCode::NotProvider
    )]
    pub provider_account: Account<'info, UserAccount>,

    /// 指定的算力提供者
    pub provider: AccountInfo<'info>,

    /// 平台账户（暂存费用，用于分佣和提现）
    #[account(
        mut,
        seeds = [b"platform"],
        bump = platform_account.bump,
    )]
    pub platform_account: Account<'info, PlatformAccount>,

    /// 开发者钱包（接收 30% 抽佣）
    #[account(
        mut,
        constraint = platform_account.dev_wallet == dev_wallet.key() @ ErrorCode::InvalidDevWallet
    )]
    pub dev_wallet: SystemAccount<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct WithdrawEarnings<'info> {
    #[account(
        mut,
        seeds = [b"user", user.key().as_ref()],
        bump = provider_account.bump,
        constraint = provider_account.owner == user.key() @ ErrorCode::Unauthorized
    )]
    pub provider_account: Account<'info, UserAccount>,

    #[account(
        mut,
        seeds = [b"platform"],
        bump = platform_account.bump,
    )]
    pub platform_account: Account<'info, PlatformAccount>,

    #[account(mut)]
    pub user: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct InitializePlatform<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + PlatformAccount::INIT_SPACE,
        seeds = [b"platform"],
        bump
    )]
    pub platform_account: Account<'info, PlatformAccount>,

    /// 开发者钱包地址（接收 30% 抽佣）
    pub dev_wallet: SystemAccount<'info>,

    #[account(mut)]
    pub authority: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct UpdateExchangeRate<'info> {
    #[account(
        mut,
        seeds = [b"platform"],
        bump = platform_account.bump,
        constraint = platform_account.authority == authority.key() @ ErrorCode::Unauthorized
    )]
    pub platform_account: Account<'info, PlatformAccount>,

    pub authority: Signer<'info>,
}

// ============ 数据结构 ============

#[account]
#[derive(InitSpace)]
pub struct UserAccount {
    pub owner: Pubkey,                       // 用户钱包地址
    pub total_tokens_consumed: u64,          // 累计消耗的 token 数
    pub total_spent: u64,                    // 累计花费 (lamports)
    pub is_provider: bool,                   // 是否为算力提供者
    pub compute_power_contributed: u64,      // 贡献的算力 (token 数)
    pub pending_revenue: u64,                // 待提现收益 (lamports, 70% 部分)
    pub withdrawn: u64,                      // 已提现总额
    pub last_withdraw_time: i64,             // 上次提现时间
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct PlatformAccount {
    pub authority: Pubkey,           // 平台管理员
    pub dev_wallet: Pubkey,          // 开发者钱包 (30% 抽佣接收地址)
    pub lamports_per_yuan: u64,      // 汇率: 1 元 = X lamports
    pub last_rate_update: i64,       // 上次汇率更新时间
    pub total_revenue: u64,          // 总收入
    pub total_dev_fees: u64,         // 总开发者抽佣
    pub total_provider_fees: u64,    // 总提供者分成
    pub total_compute_units: u64,    // 总处理 token 数
    pub total_api_calls: u64,        // 总 API 调用次数
    pub bump: u8,
}

// ============ 错误代码 ============

#[error_code]
pub enum ErrorCode {
    #[msg("已经是算力提供者")]
    AlreadyProvider,

    #[msg("不是算力提供者")]
    NotProvider,

    #[msg("没有可提现的收益")]
    NoEarnings,

    #[msg("计算溢出")]
    CalculationOverflow,

    #[msg("未授权操作")]
    Unauthorized,

    #[msg("低于最小提现金额")]
    BelowMinimumWithdraw,

    #[msg("提现冷却时间未到")]
    WithdrawCooldown,

    #[msg("平台账户余额不足")]
    InsufficientPlatformBalance,

    #[msg("无效的Token数量")]
    InvalidTokenAmount,

    #[msg("无效的开发者钱包地址")]
    InvalidDevWallet,

    #[msg("无效的汇率，范围: 100,000 ~ 100,000,000 lamports/元")]
    InvalidExchangeRate,

    #[msg("不允许自我交易（用户和提供者不能是同一人）")]
    SelfTransactionNotAllowed,

    #[msg("无效的成本金额（成本必须大于0）")]
    InvalidCostAmount,

    #[msg("用户余额不足")]
    InsufficientUserBalance,

    #[msg("无效的费用计算（分成总和超过总成本）")]
    InvalidFeeCalculation,

    #[msg("汇率更新过于频繁，请等待至少5分钟")]
    RateUpdateTooFrequent,

    #[msg("汇率变化过大，单次最多变化50%")]
    RateChangeTooLarge,
}

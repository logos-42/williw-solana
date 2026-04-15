use anchor_lang::prelude::*;

declare_id!("Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS");

// 常量定义
const MIN_WITHDRAW_AMOUNT: u64 = 1_000_000; // 0.001 SOL 最小提现金额
const WITHDRAW_COOLDOWN: i64 = 3600; // 1小时提现冷却时间
const MAX_COMPUTE_UNITS_PER_SUBMIT: u64 = 1_000_000; // 单次最大提交算力
const MAX_API_CREDITS_PER_CONSUME: u64 = 10_000; // 单次最大消耗额度

#[program]
pub mod compute_power {
    use super::*;

    /// 初始化用户账户并绑定钱包
    pub fn initialize_user(ctx: Context<InitializeUser>) -> Result<()> {
        let user_account = &mut ctx.accounts.user_account;
        user_account.owner = ctx.accounts.user.key();
        user_account.subscription_plan = SubscriptionPlan::None;
        user_account.subscription_expiry = 0;
        user_account.api_credits = 0;
        user_account.is_provider = false;
        user_account.compute_power_contributed = 0;
        user_account.earnings = 0;
        user_account.last_withdraw_time = 0;
        user_account.bump = ctx.bumps.user_account;

        msg!("用户账户初始化成功: {}", ctx.accounts.user.key());
        Ok(())
    }

    /// 订阅计划支付
    pub fn subscribe_plan(ctx: Context<SubscribePlan>, plan: SubscriptionPlan) -> Result<()> {
        let user_account = &mut ctx.accounts.user_account;
        let platform_account = &mut ctx.accounts.platform_account;

        // 验证用户账户所有者
        require!(
            user_account.owner == ctx.accounts.user.key(),
            ErrorCode::Unauthorized
        );

        // 获取订阅计划价格和API额度
        let (price, credits, duration) = match plan {
            SubscriptionPlan::Basic => (1_000_000_000, 10_000, 30 * 24 * 60 * 60), // 1 SOL, 10k credits, 30天
            SubscriptionPlan::Pro => (5_000_000_000, 100_000, 30 * 24 * 60 * 60), // 5 SOL, 100k credits, 30天
            SubscriptionPlan::Enterprise => (20_000_000_000, 500_000, 30 * 24 * 60 * 60), // 20 SOL, 500k credits, 30天
            SubscriptionPlan::None => return Err(ErrorCode::InvalidSubscriptionPlan.into()),
        };

        // 检查用户余额是否足够
        let user_balance = ctx.accounts.user.to_account_info().lamports();
        require!(user_balance >= price, ErrorCode::InsufficientBalance);

        // 转账支付 - 先转账再更新状态，防止重入攻击
        let cpi_context = CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            anchor_lang::system_program::Transfer {
                from: ctx.accounts.user.to_account_info(),
                to: platform_account.to_account_info(),
            },
        );
        anchor_lang::system_program::transfer(cpi_context, price)?;

        // 更新用户订阅信息 - 使用 checked_add 防止溢出
        user_account.subscription_plan = plan;
        let clock = Clock::get()?;
        user_account.subscription_expiry = clock
            .unix_timestamp
            .checked_add(duration)
            .ok_or(ErrorCode::CalculationOverflow)?;
        user_account.api_credits = user_account
            .api_credits
            .checked_add(credits)
            .ok_or(ErrorCode::CalculationOverflow)?;

        // 更新平台收入
        platform_account.total_revenue = platform_account
            .total_revenue
            .checked_add(price)
            .ok_or(ErrorCode::CalculationOverflow)?;

        msg!("订阅成功: 获得 {} API 额度", credits);
        Ok(())
    }

    /// 注册成为算力提供者
    pub fn register_as_provider(ctx: Context<RegisterProvider>) -> Result<()> {
        let user_account = &mut ctx.accounts.user_account;

        // 验证用户账户所有者
        require!(
            user_account.owner == ctx.accounts.user.key(),
            ErrorCode::Unauthorized
        );

        require!(!user_account.is_provider, ErrorCode::AlreadyProvider);

        user_account.is_provider = true;
        user_account.compute_power_contributed = 0;
        user_account.earnings = 0;
        user_account.last_withdraw_time = 0;

        msg!("用户 {} 注册为算力提供者", ctx.accounts.user.key());
        Ok(())
    }

    /// 提交算力贡献（由验证节点调用）
    pub fn submit_compute_work(ctx: Context<SubmitComputeWork>, compute_units: u64) -> Result<()> {
        let provider_account = &mut ctx.accounts.provider_account;
        let platform_account = &mut ctx.accounts.platform_account;

        require!(provider_account.is_provider, ErrorCode::NotProvider);

        // 验证算力单位在合理范围内，防止作弊
        require!(
            compute_units > 0 && compute_units <= MAX_COMPUTE_UNITS_PER_SUBMIT,
            ErrorCode::InvalidComputeUnits
        );

        // 验证验证节点必须是平台授权的验证器
        require!(
            platform_account.authority == ctx.accounts.validator.key(),
            ErrorCode::UnauthorizedValidator
        );

        // 计算收益：每1000个计算单位 = 0.001 SOL，使用 checked 操作防止溢出
        let earnings = (compute_units as u128)
            .checked_mul(1_000_000) // 0.001 SOL in lamports
            .and_then(|v| v.checked_div(1000))
            .ok_or(ErrorCode::CalculationOverflow)? as u64;

        // 检查平台账户是否有足够余额支付收益
        let platform_balance = platform_account.to_account_info().lamports();
        require!(
            platform_balance >= earnings,
            ErrorCode::InsufficientPlatformBalance
        );

        // 使用 checked_add 防止溢出
        provider_account.compute_power_contributed = provider_account
            .compute_power_contributed
            .checked_add(compute_units)
            .ok_or(ErrorCode::CalculationOverflow)?;

        provider_account.earnings = provider_account
            .earnings
            .checked_add(earnings)
            .ok_or(ErrorCode::CalculationOverflow)?;

        platform_account.total_compute_units = platform_account
            .total_compute_units
            .checked_add(compute_units)
            .ok_or(ErrorCode::CalculationOverflow)?;

        msg!(
            "算力提交成功: {} 单位, 收益: {} lamports",
            compute_units,
            earnings
        );
        Ok(())
    }

    /// 提现收益
    pub fn withdraw_earnings(ctx: Context<WithdrawEarnings>) -> Result<()> {
        let provider_account = &mut ctx.accounts.provider_account;
        let platform_account = &mut ctx.accounts.platform_account;

        // 验证用户账户所有者
        require!(
            provider_account.owner == ctx.accounts.user.key(),
            ErrorCode::Unauthorized
        );

        require!(provider_account.is_provider, ErrorCode::NotProvider);
        require!(provider_account.earnings > 0, ErrorCode::NoEarnings);

        // 检查最小提现金额
        require!(
            provider_account.earnings >= MIN_WITHDRAW_AMOUNT,
            ErrorCode::BelowMinimumWithdraw
        );

        // 检查提现冷却时间，防止频繁提现攻击
        let clock = Clock::get()?;
        require!(
            clock.unix_timestamp >= provider_account.last_withdraw_time + WITHDRAW_COOLDOWN,
            ErrorCode::WithdrawCooldown
        );

        let earnings = provider_account.earnings;

        // 检查平台账户余额是否足够
        let platform_balance = platform_account.to_account_info().lamports();
        require!(
            platform_balance >= earnings,
            ErrorCode::InsufficientPlatformBalance
        );

        // 使用 system_program 转账，更安全规范
        let transfer_context = CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            anchor_lang::system_program::Transfer {
                from: platform_account.to_account_info(),
                to: ctx.accounts.user.to_account_info(),
            },
        );
        anchor_lang::system_program::transfer(transfer_context, earnings)?;

        // 先更新状态再转账，防止重入攻击
        provider_account.earnings = 0;
        provider_account.last_withdraw_time = clock.unix_timestamp;

        msg!("提现成功: {} lamports", earnings);
        Ok(())
    }

    /// 消耗 API 额度
    pub fn consume_api_credits(ctx: Context<ConsumeApiCredits>, credits: u64) -> Result<()> {
        let user_account = &mut ctx.accounts.user_account;
        let platform_account = &mut ctx.accounts.platform_account;

        // 验证用户账户所有者
        require!(
            user_account.owner == ctx.accounts.user.key(),
            ErrorCode::Unauthorized
        );

        // 验证消耗额度在合理范围内
        require!(
            credits > 0 && credits <= MAX_API_CREDITS_PER_CONSUME,
            ErrorCode::InvalidCreditAmount
        );

        // 检查订阅是否有效
        let clock = Clock::get()?;
        require!(
            user_account.subscription_expiry > clock.unix_timestamp,
            ErrorCode::SubscriptionExpired
        );

        require!(
            user_account.api_credits >= credits,
            ErrorCode::InsufficientCredits
        );

        // 使用 checked_sub 防止下溢
        user_account.api_credits = user_account
            .api_credits
            .checked_sub(credits)
            .ok_or(ErrorCode::CalculationOverflow)?;

        platform_account.total_api_calls = platform_account
            .total_api_calls
            .checked_add(1)
            .ok_or(ErrorCode::CalculationOverflow)?;

        msg!(
            "消耗 {} API 额度, 剩余: {}",
            credits,
            user_account.api_credits
        );
        Ok(())
    }

    /// 初始化平台账户
    pub fn initialize_platform(ctx: Context<InitializePlatform>) -> Result<()> {
        let platform_account = &mut ctx.accounts.platform_account;
        platform_account.authority = ctx.accounts.authority.key();
        platform_account.total_revenue = 0;
        platform_account.total_compute_units = 0;
        platform_account.total_api_calls = 0;
        platform_account.bump = ctx.bumps.platform_account;

        msg!("平台账户初始化成功");
        Ok(())
    }
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
pub struct SubscribePlan<'info> {
    #[account(
        mut,
        seeds = [b"user", user.key().as_ref()],
        bump = user_account.bump,
        constraint = user_account.owner == user.key() @ ErrorCode::Unauthorized
    )]
    pub user_account: Account<'info, UserAccount>,

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
    #[account(
        mut,
        seeds = [b"user", provider.key().as_ref()],
        bump = provider_account.bump,
        constraint = provider_account.owner == provider.key() @ ErrorCode::Unauthorized
    )]
    pub provider_account: Account<'info, UserAccount>,

    #[account(
        mut,
        seeds = [b"platform"],
        bump = platform_account.bump,
    )]
    pub platform_account: Account<'info, PlatformAccount>,

    /// 验证节点或授权账户
    pub validator: Signer<'info>,

    /// CHECK: 算力提供者地址，通过 constraint 验证
    pub provider: AccountInfo<'info>,
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
pub struct ConsumeApiCredits<'info> {
    #[account(
        mut,
        seeds = [b"user", user.key().as_ref()],
        bump = user_account.bump,
        constraint = user_account.owner == user.key() @ ErrorCode::Unauthorized
    )]
    pub user_account: Account<'info, UserAccount>,

    #[account(
        mut,
        seeds = [b"platform"],
        bump = platform_account.bump,
    )]
    pub platform_account: Account<'info, PlatformAccount>,

    pub user: Signer<'info>,
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

    #[account(mut)]
    pub authority: Signer<'info>,

    pub system_program: Program<'info, System>,
}

// ============ 数据结构 ============

#[account]
#[derive(InitSpace)]
pub struct UserAccount {
    pub owner: Pubkey,                       // 用户钱包地址
    pub subscription_plan: SubscriptionPlan, // 订阅计划
    pub subscription_expiry: i64,            // 订阅到期时间
    pub api_credits: u64,                    // API 调用额度
    pub is_provider: bool,                   // 是否为算力提供者
    pub compute_power_contributed: u64,      // 贡献的算力单位
    pub earnings: u64,                       // 累计收益（lamports）
    pub last_withdraw_time: i64,             // 上次提现时间
    pub bump: u8,                            // PDA bump
}

#[account]
#[derive(InitSpace)]
pub struct PlatformAccount {
    pub authority: Pubkey,        // 平台管理员
    pub total_revenue: u64,       // 总收入
    pub total_compute_units: u64, // 总算力单位
    pub total_api_calls: u64,     // 总 API 调用次数
    pub bump: u8,                 // PDA bump
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, InitSpace, Debug)]
pub enum SubscriptionPlan {
    None,
    Basic,      // 基础版
    Pro,        // 专业版
    Enterprise, // 企业版
}

// ============ 错误代码 ============

#[error_code]
pub enum ErrorCode {
    #[msg("无效的订阅计划")]
    InvalidSubscriptionPlan,

    #[msg("已经是算力提供者")]
    AlreadyProvider,

    #[msg("不是算力提供者")]
    NotProvider,

    #[msg("没有可提现的收益")]
    NoEarnings,

    #[msg("订阅已过期")]
    SubscriptionExpired,

    #[msg("API 额度不足")]
    InsufficientCredits,

    #[msg("计算溢出")]
    CalculationOverflow,

    #[msg("未授权操作")]
    Unauthorized,

    #[msg("余额不足")]
    InsufficientBalance,

    #[msg("低于最小提现金额")]
    BelowMinimumWithdraw,

    #[msg("提现冷却时间未到")]
    WithdrawCooldown,

    #[msg("平台账户余额不足")]
    InsufficientPlatformBalance,

    #[msg("无效的算力单位数量")]
    InvalidComputeUnits,

    #[msg("未授权的验证节点")]
    UnauthorizedValidator,

    #[msg("无效的额度数量")]
    InvalidCreditAmount,
}

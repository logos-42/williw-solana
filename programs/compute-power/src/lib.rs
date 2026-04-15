use anchor_lang::prelude::*;

declare_id!("Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS");

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
        user_account.bump = ctx.bumps.user_account;

        msg!("用户账户初始化成功: {}", ctx.accounts.user.key());
        Ok(())
    }

    /// 订阅计划支付
    pub fn subscribe_plan(ctx: Context<SubscribePlan>, plan: SubscriptionPlan) -> Result<()> {
        let user_account = &mut ctx.accounts.user_account;

        // 获取订阅计划价格和API额度
        let (price, credits, duration) = match plan {
            SubscriptionPlan::Basic => (1_000_000_000, 10_000, 30 * 24 * 60 * 60), // 1 SOL, 10k credits, 30天
            SubscriptionPlan::Pro => (5_000_000_000, 100_000, 30 * 24 * 60 * 60), // 5 SOL, 100k credits, 30天
            SubscriptionPlan::Enterprise => (20_000_000_000, 500_000, 30 * 24 * 60 * 60), // 20 SOL, 500k credits, 30天
            SubscriptionPlan::None => return Err(ErrorCode::InvalidSubscriptionPlan.into()),
        };

        // 转账支付
        let cpi_context = CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            anchor_lang::system_program::Transfer {
                from: ctx.accounts.user.to_account_info(),
                to: ctx.accounts.platform_account.to_account_info(),
            },
        );
        anchor_lang::system_program::transfer(cpi_context, price)?;

        // 更新用户订阅信息
        user_account.subscription_plan = plan;
        let clock = Clock::get()?;
        user_account.subscription_expiry = clock.unix_timestamp + duration;
        user_account.api_credits += credits;

        // 更新平台收入 - 使用独立的可变借用
        ctx.accounts.platform_account.total_revenue += price;

        msg!("订阅成功: 获得 {} API 额度", credits);
        Ok(())
    }

    /// 注册成为算力提供者
    pub fn register_as_provider(ctx: Context<RegisterProvider>) -> Result<()> {
        let user_account = &mut ctx.accounts.user_account;

        require!(!user_account.is_provider, ErrorCode::AlreadyProvider);

        user_account.is_provider = true;
        user_account.compute_power_contributed = 0;
        user_account.earnings = 0;

        msg!("用户 {} 注册为算力提供者", ctx.accounts.user.key());
        Ok(())
    }

    /// 提交算力贡献（由验证节点调用）
    pub fn submit_compute_work(ctx: Context<SubmitComputeWork>, compute_units: u64) -> Result<()> {
        let provider_account = &mut ctx.accounts.provider_account;
        let platform_account = &mut ctx.accounts.platform_account;

        require!(provider_account.is_provider, ErrorCode::NotProvider);

        // 计算收益：每1000个计算单位 = 0.001 SOL
        let earnings = (compute_units as u128)
            .checked_mul(1_000_000) // 0.001 SOL in lamports
            .and_then(|v| v.checked_div(1000))
            .ok_or(ErrorCode::CalculationOverflow)? as u64;

        provider_account.compute_power_contributed += compute_units;
        provider_account.earnings += earnings;

        platform_account.total_compute_units += compute_units;

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

        require!(provider_account.is_provider, ErrorCode::NotProvider);
        require!(provider_account.earnings > 0, ErrorCode::NoEarnings);

        let earnings = provider_account.earnings;

        // 从平台账户转账到用户
        **platform_account
            .to_account_info()
            .try_borrow_mut_lamports()? -= earnings;
        **ctx
            .accounts
            .user
            .to_account_info()
            .try_borrow_mut_lamports()? += earnings;

        provider_account.earnings = 0;

        msg!("提现成功: {} lamports", earnings);
        Ok(())
    }

    /// 消耗 API 额度
    pub fn consume_api_credits(ctx: Context<ConsumeApiCredits>, credits: u64) -> Result<()> {
        let user_account = &mut ctx.accounts.user_account;
        let platform_account = &mut ctx.accounts.platform_account;

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

        user_account.api_credits -= credits;
        platform_account.total_api_calls += 1;

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

    /// CHECK: 算力提供者地址
    pub provider: AccountInfo<'info>,
}

#[derive(Accounts)]
pub struct WithdrawEarnings<'info> {
    #[account(
        mut,
        seeds = [b"user", user.key().as_ref()],
        bump = provider_account.bump,
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
}

#[derive(Accounts)]
pub struct ConsumeApiCredits<'info> {
    #[account(
        mut,
        seeds = [b"user", user.key().as_ref()],
        bump = user_account.bump,
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
}

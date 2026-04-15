# Solana Smart Contract Plan: API Consumption & Compute Marketplace

## Overview
This Solana smart contract enables a decentralized marketplace where users can:
1. Bind their Solana wallets to mobile applications
2. Subscribe to API services with a hybrid payment model (base fee + overage charges)
3. Track REST API consumption
4. Contribute their mobile device's computational power for time-based rewards
5. Settle payments and rewards automatically on-chain

## Key Features
- Wallet binding for mobile users
- Hybrid subscription model (base subscription + API overage charges)
- REST API call tracking and metering
- Time-based computational power contribution tracking
- Automatic settlement and reward distribution
- SPL token integration for payments

## Program Architecture

### Accounts
1. **UserProfile** - Stores user wallet binding, subscription status, and compute contribution stats
2. **SubscriptionPlan** - Defines available subscription tiers with base fees, API limits, and overage rates
3. **APIUsageTracker** - Records REST API consumption per user per billing period
4. **ComputeContribution** - Tracks time-based compute contributions from users
5. **RewardPool** - Manages collected fees and distributes rewards to compute contributors
6. **TransactionLedger** - Records all payment and reward transactions

### Instructions
1. `initialize_user_profile` - Bind wallet to user profile
2. `create_subscription_plan` - Admin function to define subscription tiers
3. `subscribe_to_plan` - User purchases/subscribe to a plan
4. `track_api_call` - Record a REST API call consumption
5. `start_compute_session` - Begin tracking compute contribution time
6. `end_compute_session` - End tracking and record contribution duration
7. `process_billing_cycle` - Calculate overage charges and distribute rewards
8. `withdraw_earnings` - Allow users to withdraw accumulated rewards
9. `update_subscription` - Change subscription tier
10. `cancel_subscription` - End subscription service

## Data Structures

### UserProfile
```rust
pub struct UserProfile {
    pub wallet_address: Pubkey,      // User's Solana wallet
    pub is_active: bool,             // Account status
    pub subscription_plan: Option<Pubkey>, // Linked subscription plan
    pub subscription_start: i64,     // Unix timestamp
    pub subscription_end: i64,       // Unix timestamp
    pub total_api_calls: u64,        // Lifetime API calls
    pub total_compute_hours: u64,    // Lifetime compute contribution (in hours)
    pub pending_rewards: u64,        // Unwithdrawn rewards (in lamports)
    pub last_billing: i64,           // Last billing cycle timestamp
}
```

### SubscriptionPlan
```rust
pub struct SubscriptionPlan {
    pub admin_authority: Pubkey,     // Authority to modify plan
    pub name: String,                // Plan name (e.g., "Basic", "Pro")
    pub base_fee_lamports: u64,      // Monthly base fee
    pub included_api_calls: u64,     // API calls included in base fee
    pub overage_rate_per_call: u64,  // Lamports per API call over limit
    pub is_active: bool,             // Plan availability
}
```

### APIUsageTracker
```rust
pub struct APIUsageTracker {
    pub user_profile: Pubkey,        // Linked user profile
    pub period_start: i64,           // Billing period start
    pub period_end: i64,             // Billing period end
    pub api_calls_count: u64,        // API calls in this period
}
```

### ComputeContribution
```rust
pub struct ComputeContribution {
    pub user_profile: Pubkey,        // Linked user profile
    pub session_start: i64,          // Session start timestamp
    pub session_end: Option<i64>,    // Session end timestamp (None if active)
    pub duration_seconds: u64,       // Contribution duration
    pub reward_rate_per_hour: u64,   // Reward rate (lamports/hour)
    pub is_settled: bool,            // Whether reward has been calculated
}
```

### RewardPool
```rust
pub struct RewardPool {
    pub admin_authority: Pubkey,     // Authority to manage pool
    pub total_collected: u64,        // Total fees collected (lamports)
    pub total_distributed: u64,      // Total rewards distributed (lamports)
    pub reward_rate_per_compute_hour: u64, // Current reward rate
    pub last_distribution: i64,      // Last reward distribution timestamp
}
```

## Workflow Flows

### 1. User Onboarding & Wallet Binding
1. User opens mobile app and initiates wallet binding
2. Mobile app calls `initialize_user_profile` with user's wallet address
3. Contract creates UserProfile account linked to wallet
4. User now has an on-chain identity for subscription and tracking

### 2. Subscription Management
1. Admin creates subscription plans via `create_subscription_plan`
2. User selects plan and calls `subscribe_to_plan`
3. Contract transfers base fee to RewardPool and sets subscription dates
4. User gains access to API services according to plan limits

### 3. API Consumption Tracking
1. Mobile app makes REST API calls to service
2. After each call, app calls `track_api_call` 
3. Contract increments API counter in user's current APIUsageTracker
4. At billing cycle end, overage charges calculated and collected

### 4. Compute Contribution
1. User opts to contribute compute power in mobile app
2. App calls `start_compute_session` to begin tracking
3. App periodically updates session or calls `end_compute_session` when done
4. Contract records duration and calculates pending rewards
5. Rewards distributed during billing cycle processing

### 5. Billing & Settlement
1. At period end (or on demand), `process_billing_cycle` called
2. For each user:
   - Calculate API overage: max(0, (calls - included) * overage_rate)
   - Add to user's pending rewards owed (negative = user owes)
   - Calculate compute rewards: duration_seconds * reward_rate_per_hour / 3600
   - Add to user's pending rewards owed (positive = user earns)
3. Update RewardPool balances
4. Prepare for next billing cycle

### 6. Withdrawals
1. User calls `withdraw_earnings` when pending_rewards > 0
2. Contract transfers lamports from RewardPool to user's wallet
3. Updates UserProfile pending_rewards to zero
4. Records transaction in TransactionLedger

## Security Considerations
1. **Access Control** - Only authorized users can modify their own data
2. **Reentrancy Protection** - Use Anchor's reentrancy guards or manual checks
3. **Integer Overflow** - Use checked math operations
4. **Time Manipulation** - Use Unix timestamps cautiously; consider using Slot for critical timing
5. **Admin Privileges** - Multisig or timelock for critical admin functions
6. **Input Validation** - Validate all incoming data (amounts, durations, etc.)
7. **Session Hijacking** - Ensure compute sessions are tied to user profiles

## Integration Points
1. **Mobile App SDK** - Functions to call contract instructions:
   - bind_wallet()
   - subscribe_plan(plan_id)
   - track_api_call()
   - start_compute_session()
   - end_compute_session()
   - withdraw_earnings()

2. **API Gateway** - Middleware to verify subscription and track calls:
   - Verify user has active subscription
   - Call contract's track_api_call after each API request
   - Enforce rate limits based on subscription tier

3. **Reward Distribution** - External service or DAO to:
   - Monitor RewardPool balances
   - Adjust reward rates based on market conditions
   - Propose and vote on parameter changes

## Implementation Technology Stack
- **Language**: Rust
- **Framework**: Anchor (recommended for safer development)
- **Blockchain**: Solana Mainnet Beta or Devnet for testing
- **Token System**: SOL (lamports) for simplicity; could extend to SPL tokens
- **Testing**: Anchor test framework, Solana program test
- **Deployment**: Solana CLI, Anchor deploy

## Error Handling
Custom error codes for:
- Unauthorized access
- Invalid subscription state
- Insufficient funds for subscription
- Compute session already active
- API call tracking outside subscription period
- Invalid time sequences (end before start)
- Withdrawal attempts with zero balance
- Account initialization conflicts

## Upgradeability Considerations
If upgradeable contract is needed:
- Use Solana's upgradeable program infrastructure
- Implement data migration strategies for account structure changes
- Consider using the Solana Native Upgradeable Loader

## Testing Strategy
1. Unit tests for each instruction
2. Integration tests for complete workflows
3. Fuzz testing for edge cases
4. Simulation of billing cycles with various usage patterns
5. Security auditing focus on reentrancy and access controls

## Deployment Plan
1. Develop and test on Solana Devnet
2. Conduct security audit
3. Deploy to Solana Testnet for beta testing
4. Monitor and collect feedback
5. Deploy to Solana Mainnet
6. Implement monitoring and alerting for production

## Future Enhancements
1. Support for multiple tokens (USDC, etc.) via SPL token integration
2. Dynamic pricing based on supply/demand of compute resources
3. Reputation system for reliable compute contributors
4. API quality of service tiers
5. Governance mechanism for parameter updates
6. Mobile app attestation to verify genuine compute contributions
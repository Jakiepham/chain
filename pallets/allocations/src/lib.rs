#![cfg_attr(not(feature = "std"), no_std)]

//! A runtime module to handle Nodle Cash allocations to network
//! contributors, has a list of oracles that can submit Merkle
//! Root Hashes to be paid for.

use frame_support::traits::{ChangeMembers, Currency, Imbalance, InitializeMembers, OnUnbalanced};
use frame_support::{
    decl_error, decl_event, decl_module, decl_storage, dispatch::DispatchResult, ensure,
};
use sp_runtime::traits::CheckedSub;
use sp_std::prelude::Vec;
use system::ensure_signed;

type BalanceOf<T> = <<T as Trait>::Currency as Currency<<T as system::Trait>::AccountId>>::Balance;
type PositiveImbalanceOf<T> =
    <<T as Trait>::Currency as Currency<<T as system::Trait>::AccountId>>::PositiveImbalance;

/// The module's configuration trait.
pub trait Trait: system::Trait {
    /// The overarching event type.
    type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;

    // Currency minting
    type Currency: Currency<Self::AccountId>;
    type Reward: OnUnbalanced<PositiveImbalanceOf<Self>>;
}

decl_error! {
    pub enum Error for Module<T: Trait> {
        /// Function is restricted to oracles only
        OracleAccessDenied,
        /// We are trying to allocate 0 coins
        ZeroAllocation,
        /// We are trying to allocate more coins than we can
        TooManyCoinsToAllocate,
    }
}

decl_storage! {
    trait Store for Module<T: Trait> as AllocationsModule {
        Oracles get(oracles): Vec<T::AccountId>;
        CoinsLeft get(coins_left) config(): BalanceOf<T>;
    }
}

decl_module! {
    /// The module declaration.
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {
        type Error = Error<T>;
        fn deposit_event() = default;

        // As an oracle, submit a merkle root for reward
        pub fn submit_reward(origin, merkle_root_hash: T::Hash, who: T::AccountId, amount: BalanceOf<T>) -> DispatchResult {
            Self::ensure_oracle(origin)?;

            ensure!(amount > 0.into(), Error::<T>::ZeroAllocation);
            ensure!(<CoinsLeft<T>>::get() >= amount, Error::<T>::TooManyCoinsToAllocate);

            // Record the coins as spent
            <CoinsLeft<T>>::put(
                <CoinsLeft<T>>::get().checked_sub(&amount).ok_or("Underflow computing coins left")?
            );

            let mut total_imbalance = <PositiveImbalanceOf<T>>::zero();
            let r = T::Currency::deposit_creating(&who, amount);
            total_imbalance.subsume(r);
            T::Reward::on_unbalanced(total_imbalance);

            Self::deposit_event(RawEvent::RewardAllocated(who, amount, merkle_root_hash));

            Ok(())
        }
    }
}

decl_event!(
    pub enum Event<T>
    where
        AccountId = <T as system::Trait>::AccountId,
        Balance = BalanceOf<T>,
        Hash = <T as system::Trait>::Hash,
    {
        /// Some rewards were allocated to a network contributor.
        RewardAllocated(AccountId, Balance, Hash),
    }
);

impl<T: Trait> Module<T> {
    pub fn is_oracle(who: T::AccountId) -> bool {
        Self::oracles().contains(&who)
    }

    fn ensure_oracle(origin: T::Origin) -> DispatchResult {
        let sender = ensure_signed(origin)?;
        ensure!(Self::is_oracle(sender), Error::<T>::OracleAccessDenied);

        Ok(())
    }
}

impl<T: Trait> ChangeMembers<T::AccountId> for Module<T> {
    fn change_members_sorted(
        _incoming: &[T::AccountId],
        _outgoing: &[T::AccountId],
        new: &[T::AccountId],
    ) {
        <Oracles<T>>::put(new);
    }
}

impl<T: Trait> InitializeMembers<T::AccountId> for Module<T> {
    fn initialize_members(init: &[T::AccountId]) {
        <Oracles<T>>::put(init);
    }
}

/// tests for this module
#[cfg(test)]
mod tests {
    use super::*;

    use frame_support::{
        assert_noop, assert_ok, impl_outer_origin, parameter_types, weights::Weight,
    };
    use sp_core::H256;
    use sp_runtime::{
        testing::Header,
        traits::{BlakeTwo256, IdentityLookup},
        Perbill,
    };

    impl_outer_origin! {
        pub enum Origin for Test {}
    }

    // For testing the module, we construct most of a mock runtime. This means
    // first constructing a configuration type (`Test`) which `impl`s each of the
    // configuration traits of modules we want to use.
    #[derive(Clone, Eq, PartialEq)]
    pub struct Test;
    parameter_types! {
        pub const BlockHashCount: u64 = 250;
        pub const MaximumBlockWeight: Weight = 1024;
        pub const MaximumBlockLength: u32 = 2 * 1024;
        pub const AvailableBlockRatio: Perbill = Perbill::from_percent(75);
    }
    impl system::Trait for Test {
        type Origin = Origin;
        type Call = ();
        type Index = u64;
        type BlockNumber = u64;
        type Hash = H256;
        type Hashing = BlakeTwo256;
        type AccountId = u64;
        type Lookup = IdentityLookup<Self::AccountId>;
        type Header = Header;
        type Event = ();
        type BlockHashCount = BlockHashCount;
        type MaximumBlockWeight = MaximumBlockWeight;
        type MaximumBlockLength = MaximumBlockLength;
        type AvailableBlockRatio = AvailableBlockRatio;
        type Version = ();
        type ModuleToIndex = ();
    }
    impl balances::Trait for Test {
        type Balance = u64;
        type OnNewAccount = ();
        type OnReapAccount = ();
        type Event = ();
        type TransferPayment = ();
        type DustRemoval = ();
        type ExistentialDeposit = ();
        type CreationFee = ();
    }
    impl Trait for Test {
        type Event = ();

        type Currency = balances::Module<Self>;
        type Reward = ();
    }
    type AllocationsModule = Module<Test>;

    pub type Balances = balances::Module<Test>;

    pub const ORACLE: u64 = 0;
    pub const NON_ORACLE: u64 = 1;

    pub const INITIAL_COINS: u64 = 200;
    pub const REWARD_TARGET: u64 = 2;
    pub const REWARD_AMOUNT: u64 = 100;

    // This function basically just builds a genesis storage key/value store according to
    // our desired mockup.
    fn new_test_ext() -> sp_io::TestExternalities {
        GenesisConfig::<Test> {
            coins_left: INITIAL_COINS,
        }
        .build_storage()
        .unwrap()
        .into()
    }

    #[test]
    fn non_oracle_can_not_submit_reward() {
        new_test_ext().execute_with(|| {
            assert_noop!(
                AllocationsModule::submit_reward(
                    Origin::signed(NON_ORACLE),
                    H256::random(),
                    REWARD_TARGET,
                    REWARD_AMOUNT
                ),
                Error::<Test>::OracleAccessDenied
            );
        })
    }

    #[test]
    fn oracle_submit_reward() {
        new_test_ext().execute_with(|| {
            AllocationsModule::initialize_members(&[ORACLE]);
            assert_eq!(AllocationsModule::is_oracle(ORACLE), true);

            assert_eq!(Balances::free_balance(REWARD_TARGET), 0);
            assert_ok!(AllocationsModule::submit_reward(
                Origin::signed(ORACLE),
                H256::random(),
                REWARD_TARGET,
                REWARD_AMOUNT
            ));
            assert_eq!(Balances::free_balance(REWARD_TARGET), REWARD_AMOUNT);

            // Record coins left
            assert_eq!(
                AllocationsModule::coins_left(),
                INITIAL_COINS - REWARD_AMOUNT
            );
        })
    }

    #[test]
    fn oracle_management_initialize_members() {
        new_test_ext().execute_with(|| {
            // Start with no oracles
            assert_eq!(AllocationsModule::is_oracle(ORACLE), false);
            AllocationsModule::initialize_members(&[ORACLE]);
            assert_eq!(AllocationsModule::is_oracle(ORACLE), true);
        })
    }

    #[test]
    fn oracle_management_change_members_sorted_remove_oracle() {
        new_test_ext().execute_with(|| {
            AllocationsModule::initialize_members(&[ORACLE]);

            AllocationsModule::change_members_sorted(&[], &[], &[]);
            assert_eq!(AllocationsModule::is_oracle(ORACLE), false);
        })
    }

    #[test]
    fn oracle_management_change_members_sorted_add_oracle() {
        new_test_ext().execute_with(|| {
            AllocationsModule::initialize_members(&[ORACLE]);

            AllocationsModule::change_members_sorted(&[], &[], &[ORACLE, NON_ORACLE]);
            assert_eq!(AllocationsModule::is_oracle(ORACLE), true);
            assert_eq!(AllocationsModule::is_oracle(NON_ORACLE), true);
        })
    }

    #[test]
    fn oracle_management_change_members_sorted_swap_oracle() {
        new_test_ext().execute_with(|| {
            AllocationsModule::initialize_members(&[ORACLE]);

            AllocationsModule::change_members_sorted(&[], &[], &[NON_ORACLE]);
            assert_eq!(AllocationsModule::is_oracle(ORACLE), false);
            assert_eq!(AllocationsModule::is_oracle(NON_ORACLE), true);
        })
    }

    #[test]
    fn cannot_allocate_zero_coins() {
        new_test_ext().execute_with(|| {
            AllocationsModule::initialize_members(&[ORACLE]);

            assert_noop!(
                AllocationsModule::submit_reward(
                    Origin::signed(ORACLE),
                    H256::random(),
                    REWARD_TARGET,
                    0
                ),
                Error::<Test>::ZeroAllocation
            );
        })
    }

    #[test]
    fn cannot_allocate_more_than_coins_left() {
        new_test_ext().execute_with(|| {
            AllocationsModule::initialize_members(&[ORACLE]);

            assert_noop!(
                AllocationsModule::submit_reward(
                    Origin::signed(ORACLE),
                    H256::random(),
                    REWARD_TARGET,
                    INITIAL_COINS + 1
                ),
                Error::<Test>::TooManyCoinsToAllocate
            );
        })
    }
}

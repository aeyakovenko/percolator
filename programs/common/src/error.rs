//! Error types

/// Program errors
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PercolatorError {
    // Common errors (0-99)
    InvalidInstruction = 0,
    InvalidAccount = 1,
    InvalidAccountOwner = 2,
    InvalidMint = 3,
    InsufficientFunds = 4,
    Overflow = 5,
    Underflow = 6,
    Unauthorized = 7,

    // Router errors (100-199)
    InvalidSlab = 100,
    SlabNotRegistered = 101,
    SlabVersionMismatch = 102,
    CapExpired = 103,
    CapInvalidScope = 104,
    CapInsufficientRemaining = 105,
    EscrowInsufficientBalance = 106,
    PortfolioInsufficientMargin = 107,
    InvalidPortfolio = 108,
    CpiFailed = 109,
    PortfolioHealthy = 110,
    LiquidationCooldown = 111,
    InvalidAmount = 112,
    InsufficientBalance = 113,
    StalePrice = 114,
    AlreadyInitialized = 115,

    // Slab errors (200-299)
    InvalidInstrument = 200,
    InvalidOrder = 201,
    InvalidReservation = 202,
    ReservationExpired = 203,
    ReservationNotFound = 204,
    OrderNotFound = 205,
    PositionNotFound = 206,
    InsufficientLiquidity = 207,
    PriceNotAligned = 208,
    QuantityNotAligned = 209,
    InvalidPrice = 210,
    InvalidQuantity = 211,
    PoolFull = 212,
    SeqnoMismatch = 213,

    // Matching errors (300-399)
    InvalidSide = 300,
    InvalidTimeInForce = 301,
    InvalidMakerClass = 302,
    InvalidOrderState = 303,
    BookCorrupted = 304,
    ReservedQtyExceeded = 305,

    // Risk errors (400-499)
    InsufficientMargin = 400,
    BelowMaintenanceMargin = 401,
    InvalidRiskParams = 402,

    // Anti-toxicity errors (500-599)
    KillBandExceeded = 500,
    OrderFrozen = 501,
    BatchNotOpen = 502,
    InvalidCommitment = 503,
    JitPenaltyApplied = 504,
    RoundtripDetected = 505,
}

impl From<PercolatorError> for u64 {
    fn from(e: PercolatorError) -> u64 {
        e as u64
    }
}

impl From<PercolatorError> for pinocchio::program_error::ProgramError {
    fn from(e: PercolatorError) -> Self {
        pinocchio::program_error::ProgramError::from(e as u64)
    }
}

use crate::{
    validate_version_specific_fields, EngineApiMessageVersion, EngineObjectValidationError,
};
use alloy_primitives::{Address, B256, U256};
use reth_chain_state::ExecutedBlock;
use reth_chainspec::ChainSpec;
use reth_primitives::{SealedBlock, Withdrawals};
use reth_rpc_types::{
    engine::{PayloadAttributes as EthPayloadAttributes, PayloadId},
    optimism::OptimismPayloadAttributes,
    Withdrawal,
};

/// Represents a built payload type that contains a built [`SealedBlock`] and can be converted into
/// engine API execution payloads.
pub trait BuiltPayload: Send + Sync + std::fmt::Debug {
    /// Returns the built block (sealed)
    fn block(&self) -> &SealedBlock;

    /// Returns the fees collected for the built block
    fn fees(&self) -> U256;

    /// Returns the entire execution data for the built block, if available.
    fn executed_block(&self) -> Option<ExecutedBlock> {
        None
    }
}

/// This can be implemented by types that describe a currently running payload job.
///
/// This is used as a conversion type, transforming a payload attributes type that the engine API
/// receives, into a type that the payload builder can use.
pub trait PayloadBuilderAttributes: Send + Sync + std::fmt::Debug {
    /// The payload attributes that can be used to construct this type. Used as the argument in
    /// [`PayloadBuilderAttributes::try_new`].
    type RpcPayloadAttributes;
    /// The error type used in [`PayloadBuilderAttributes::try_new`].
    type Error: std::error::Error;

    /// Creates a new payload builder for the given parent block and the attributes.
    ///
    /// Derives the unique [`PayloadId`] for the given parent and attributes
    fn try_new(
        parent: B256,
        rpc_payload_attributes: Self::RpcPayloadAttributes,
    ) -> Result<Self, Self::Error>
    where
        Self: Sized;

    /// Returns the [`PayloadId`] for the running payload job.
    fn payload_id(&self) -> PayloadId;

    /// Returns the parent block hash for the running payload job.
    fn parent(&self) -> B256;

    /// Returns the timestamp for the running payload job.
    fn timestamp(&self) -> u64;

    /// Returns the parent beacon block root for the running payload job, if it exists.
    fn parent_beacon_block_root(&self) -> Option<B256>;

    /// Returns the suggested fee recipient for the running payload job.
    fn suggested_fee_recipient(&self) -> Address;

    /// Returns the prevrandao field for the running payload job.
    fn prev_randao(&self) -> B256;

    /// Returns the withdrawals for the running payload job.
    fn withdrawals(&self) -> &Withdrawals;
}

/// The execution payload attribute type the CL node emits via the engine API.
/// This trait should be implemented by types that could be used to spawn a payload job.
///
/// This type is emitted as part of the forkchoiceUpdated call
pub trait PayloadAttributes:
    serde::de::DeserializeOwned + serde::Serialize + std::fmt::Debug + Clone + Send + Sync + 'static
{
    /// Returns the timestamp to be used in the payload job.
    fn timestamp(&self) -> u64;

    /// Returns the withdrawals for the given payload attributes.
    fn withdrawals(&self) -> Option<&Vec<Withdrawal>>;

    /// Return the parent beacon block root for the payload attributes.
    fn parent_beacon_block_root(&self) -> Option<B256>;

    /// Ensures that the payload attributes are valid for the given [`ChainSpec`] and
    /// [`EngineApiMessageVersion`].
    fn ensure_well_formed_attributes(
        &self,
        chain_spec: &ChainSpec,
        version: EngineApiMessageVersion,
    ) -> Result<(), EngineObjectValidationError>;
}

impl PayloadAttributes for EthPayloadAttributes {
    fn timestamp(&self) -> u64 {
        self.timestamp
    }

    fn withdrawals(&self) -> Option<&Vec<Withdrawal>> {
        self.withdrawals.as_ref()
    }

    fn parent_beacon_block_root(&self) -> Option<B256> {
        self.parent_beacon_block_root
    }

    fn ensure_well_formed_attributes(
        &self,
        chain_spec: &ChainSpec,
        version: EngineApiMessageVersion,
    ) -> Result<(), EngineObjectValidationError> {
        validate_version_specific_fields(chain_spec, version, self.into())
    }
}

impl PayloadAttributes for OptimismPayloadAttributes {
    fn timestamp(&self) -> u64 {
        self.payload_attributes.timestamp
    }

    fn withdrawals(&self) -> Option<&Vec<Withdrawal>> {
        self.payload_attributes.withdrawals.as_ref()
    }

    fn parent_beacon_block_root(&self) -> Option<B256> {
        self.payload_attributes.parent_beacon_block_root
    }

    fn ensure_well_formed_attributes(
        &self,
        chain_spec: &ChainSpec,
        version: EngineApiMessageVersion,
    ) -> Result<(), EngineObjectValidationError> {
        validate_version_specific_fields(chain_spec, version, self.into())?;

        if self.gas_limit.is_none() {
            return Err(EngineObjectValidationError::InvalidParams(
                "MissingGasLimitInPayloadAttributes".to_string().into(),
            ))
        }

        Ok(())
    }
}

/// A builder that can return the current payload attribute.
pub trait PayloadAttributesBuilder: std::fmt::Debug + Send + Sync + 'static {
    /// The payload attributes type returned by the builder.
    type PayloadAttributes: PayloadAttributes;
    /// The error type returned by [`PayloadAttributesBuilder::build`].
    type Error: std::error::Error + Send + Sync;

    /// Return a new payload attribute from the builder.
    fn build(&self) -> Result<Self::PayloadAttributes, Self::Error>;
}

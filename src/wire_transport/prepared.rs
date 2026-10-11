/********************************************************************************
 * Copyright (c) 2026 Contributors to the Eclipse Foundation
 *
 * See the NOTICE file(s) distributed with this work for additional
 * information regarding copyright ownership.
 *
 * This program and the accompanying materials are made available under the
 * terms of the Apache License Version 2.0 which is available at
 * https://www.apache.org/licenses/LICENSE-2.0
 *
 * SPDX-License-Identifier: Apache-2.0
 ********************************************************************************/
//! Prepared routes and generic receive registrations for selected wires.

use super::*;

#[cfg(all(feature = "zero-copy-transport", feature = "transport-implementer-api"))]
impl<C: crate::PreparedTransportCore, W: UWire>
    UWireTransport<Arc<C>, W, UFrameFieldsMetadataCodec>
{
    /// Prepares exact in-place metadata admission while leaving payload bytes opaque.
    ///
    /// # Errors
    /// Rejects an invalid prefix or token limit. Custom wire alias callbacks are
    /// not applied by this exact-identity admission API.
    pub fn prepare_header_receiver(
        &self,
        limits: crate::HeaderLimits,
    ) -> Result<crate::HeaderAdmission, UWireError> {
        crate::HeaderAdmission::for_wire::<W>(limits)
    }

    /// Prepares the selected-wire prefix for bounded, explicitly copying opaque routes.
    ///
    /// # Errors
    /// Rejects an invalid selected-wire prefix.
    pub fn prepare_opaque_template(&self) -> Result<crate::OpaqueTemplate, crate::SetupError> {
        crate::OpaqueTemplate::for_wire::<W>()
    }

    /// Attaches an exact-identity opaque handler with decoded source/sink filters.
    ///
    /// Each registration reserves one generic worker. Partial filters are retained
    /// at the core boundary and rechecked after in-place decoding; None is sinkless.
    ///
    /// # Errors
    /// Reports prefix/limit, worker reservation or physical subscription failure.
    pub async fn register_header_handler<H: crate::HeaderHandler<C::Rx>>(
        &self,
        source: &UUri,
        sink: Option<&UUri>,
        limits: crate::HeaderLimits,
        config: crate::RegistrationConfig,
        handler: H,
    ) -> Result<crate::WireRegistration<C>, crate::SetupError> {
        let admission = self
            .prepare_header_receiver(limits)
            .map_err(|_| crate::SetupError::Prefix)?;
        let worker = admission
            .register_filtered(source.clone(), sink.cloned(), config, handler, || {})
            .map_err(|_| crate::SetupError::Worker)?;
        crate::WireRegistration::attach(Arc::clone(&self.core), source, sink, worker).await
    }
}

#[cfg(all(feature = "zero-copy-transport", feature = "transport-implementer-api"))]
impl<C> UWireTransport<Arc<C>, StableContainerWire, UFrameFieldsMetadataCodec>
where
    C: crate::PreparedTransportCore,
{
    /// Prepares a stable sender's identity and prefix for route creation.
    ///
    /// # Errors
    /// Rejects a missing agreement or an invalid/unagreed payload representation.
    pub fn prepare_stable_template<P: crate::StablePayload>(
        &self,
    ) -> Result<crate::TxTemplate<P>, crate::SetupError> {
        let agreement = self
            .native_agreement
            .clone()
            .ok_or(crate::SetupError::InvalidConfig(
                "native agreement required",
            ))?;
        crate::TxTemplate::new(agreement)
    }

    /// Creates one fixed stable-native route from the core's reserved tables.
    ///
    /// # Errors
    /// Returns representation, route/configuration or table-capacity failures.
    pub fn prepare_stable_route<P: crate::StablePayload>(
        &self,
        definition: crate::RouteDefinition,
        config: crate::RouteConfig,
    ) -> Result<crate::TxRoute<C, P>, crate::SetupError> {
        self.prepare_stable_template()?
            .route(Arc::clone(&self.core), definition, config)
    }

    /// Prepares in-place stable receive admission using this adapter's agreement.
    ///
    /// # Errors
    /// Rejects a missing agreement or incompatible stable representation.
    pub fn prepare_stable_receiver<P: crate::StablePayload>(
        &self,
        limits: crate::HeaderLimits,
    ) -> Result<crate::RxAdmission<P>, UWireError> {
        let agreement = self
            .native_agreement
            .clone()
            .ok_or_else(|| UWireError::invalid_payload("native agreement required"))?;
        crate::RxAdmission::new(agreement, limits)
    }

    /// Attaches one typed worker with retained agreement and decoded URI filters.
    ///
    /// # Errors
    /// Reports unagreed representation, worker setup or physical subscription failure.
    pub async fn register_stable_handler<P, H>(
        &self,
        source: &UUri,
        sink: Option<&UUri>,
        limits: crate::HeaderLimits,
        config: crate::RegistrationConfig,
        handler: H,
    ) -> Result<crate::WireRegistration<C>, crate::SetupError>
    where
        P: crate::StablePayload,
        H: crate::ZeroCopyHandler<P, C::Rx>,
    {
        let admission = self
            .prepare_stable_receiver::<P>(limits)
            .map_err(|_| crate::SetupError::UnknownPayloadType)?;
        let worker = admission
            .register_filtered(source.clone(), sink.cloned(), config, handler, || {})
            .map_err(|_| crate::SetupError::Worker)?;
        crate::WireRegistration::attach(Arc::clone(&self.core), source, sink, worker).await
    }
}

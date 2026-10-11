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
//! Ownership of a prepared core's physical subscription and generic worker.

use crate::{PreparedTransportCore, Registration, RegistrationStopError, SetupError, UUri};
use std::sync::Arc;

/// Failure while detaching physical delivery or draining a registration worker.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum WireRegistrationStopError {
    /// The binding could not detach its physical subscription.
    #[error("wire registration detachment failed: {0}")]
    Detach(SetupError),
    /// The application worker panicked or was cancelled.
    #[error("wire registration worker failed: {0}")]
    Worker(RegistrationStopError),
}

/// Setup-owned physical subscription and one ordered generic worker.
///
/// Admission closes before detachment, releasing blocked producers. Explicit
/// detach errors retain the handle for retry. Stop drains previously admitted
/// frames; Drop cancels the worker. Independently retained frames are never revoked.
pub struct WireRegistration<C: PreparedTransportCore> {
    core: Arc<C>,
    subscription: Option<C::Subscription>,
    registration: Option<Registration<C::Rx>>,
}

impl<C: PreparedTransportCore> std::fmt::Debug for WireRegistration<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WireRegistration")
            .field("attached", &self.subscription.is_some())
            .finish_non_exhaustive()
    }
}

impl<C: PreparedTransportCore> WireRegistration<C> {
    /// Attaches an existing generic worker to a binding's physical filters.
    ///
    /// The worker must apply any required decoded filters and representation
    /// checks; physical filters are hints, not semantic metadata authority.
    ///
    /// # Errors
    /// Reports physical setup failure. Failure/cancellation drops the supplied
    /// registration, closing admission and aborting its worker.
    pub async fn attach(
        core: Arc<C>,
        source: &UUri,
        sink: Option<&UUri>,
        registration: Registration<C::Rx>,
    ) -> Result<Self, SetupError> {
        let subscription = core.attach(source, sink, registration.inbox()).await?;
        Ok(Self {
            core,
            subscription: Some(subscription),
            registration: Some(registration),
        })
    }

    /// Closes admission and detaches once, retaining a failed handle for retry.
    ///
    /// Already admitted work may finish. Call stop to await its drain.
    ///
    /// # Errors
    /// Reports the binding's detachment error without losing its handle.
    pub fn detach(&mut self) -> Result<(), SetupError> {
        if let Some(registration) = &self.registration {
            registration.inbox().close();
        }
        if let Some(subscription) = &mut self.subscription {
            self.core.detach(subscription)?;
        }
        self.subscription.take();
        Ok(())
    }

    /// Closes and detaches physical delivery, then waits for the worker to drain.
    ///
    /// Cancelling this future drops the registration and cancels the worker. A
    /// handler stopping itself returns after detachment, then drains in its own task.
    ///
    /// # Errors
    /// Reports detachment or worker failure. On detachment failure, admission
    /// remains closed and the worker still drains; Drop retries binding detachment.
    pub async fn stop(mut self) -> Result<(), WireRegistrationStopError> {
        let detached = self.detach();
        let drained = if let Some(worker) = self.registration.take() {
            worker.stop().await
        } else {
            Ok(())
        };
        detached.map_err(WireRegistrationStopError::Detach)?;
        drained.map_err(WireRegistrationStopError::Worker)
    }
}

impl<C: PreparedTransportCore> Drop for WireRegistration<C> {
    fn drop(&mut self) {
        let _ = self.detach();
    }
}

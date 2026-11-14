//! CP-R1 regression: no invalid metadata read; observe refusal before certification.
#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]
extern crate std;
use super::*;
use crate::communication::{two_mode::Publisher, LocalUriProvider};
use crate::memory::StartupRegion;
use crate::transport::envelope::CustomEnvelope;
use crate::frame::header::{HeaderLimits, RxHeader};
use crate::transport::tests::{NullHandoff, StaticFrame, TestUri, TestWire, CLOCK};
use crate::transport::{LoanError, PreparedTxStorage};
use core::sync::atomic::{AtomicUsize, Ordering as Order};
use std::boxed::Box;
use std::panic::{catch_unwind, AssertUnwindSafe};

struct Head { first: usize, second: usize, returned: usize, action: u8, calls: AtomicUsize }
impl CustomEnvelope for Head {
    fn name(&self) -> &'static str { "cp-r1-adversarial" }
    fn max_head_len(&self, _: usize) -> usize { 16 }
    fn head_len(&self, _: &TxHeaderParts<'_>, _: Option<usize>) -> Result<usize, UFrameHeaderError> {
        Ok(if self.calls.fetch_add(1, Order::Relaxed)==0 {self.first} else {self.second})
    }
    fn write_head(&self, _: &TxHeaderParts<'_>, _: Option<usize>, out: &mut [u8]) -> Result<usize,UFrameHeaderError> {
        // The safe callback only ever gets the initialized zeroed prefix.
        assert!(out.iter().all(|byte| *byte==0));
        if self.action==1 { return Err(UFrameHeaderError::Invalid("injected refusal")); }
        if self.action==2 { panic!("injected safe callback unwind"); }
        Ok(self.returned)
    }
    fn parse<'a>(&self, _: &'a [u8], _: PayloadBytes<'a>, _: &HeaderLimits) -> Result<(RxHeader<'a>,PayloadBytes<'a>),UFrameHeaderError> {
        Err(UFrameHeaderError::Invalid("no inbound path in this fixture"))
    }
}
struct Setup { envelopes: &'static [Envelope] }
impl BindingSetup for Setup {
    type Running=NullHandoff;
    fn name(&self)->&'static str {"cp-r1-setup"}
    fn capabilities(&self)->Capabilities {
        let mut c=NullHandoff.capabilities();c.envelopes=self.envelopes;c.default_envelope=self.envelopes[0];c
    }
    fn qos_support(&self,k:RouteKind)->QosSupport {NullHandoff.qos_support(k)}
    fn prepare_route(&mut self,r:&RouteDeclaration<'_>)->Result<PreparedRoute,BindingRefusal>{NullHandoff.prepare_route(r)}
    fn into_running(self,_:RuntimePort)->Result<NullHandoff,BindingRefusal>{Ok(NullHandoff)}
}
#[derive(Default)]
struct Probe { loans:AtomicUsize, records:AtomicUsize, published:AtomicUsize, returned:AtomicUsize }
struct Loan<'a> { metadata:Box<[MaybeUninit<u8>]>, payload:Option<Box<[MaybeUninit<u8>]>>, probe:&'a Probe }
// SAFETY: each Box owns a distinct allocation exclusively; moving Loan does not
// move either backing allocation. Accessors borrow only their own disjoint Box.
// No method resizes, replaces, deinitializes or aliases the storage; record_header
// checks the unchanged full metadata extent. Safe ownership excludes concurrent
// access/release. Drop destroys only MaybeUninit storage, reading no byte; Send
// follows Box and the shared atomic-only Probe. Zero extents use valid Box slices.
unsafe impl PreparedTxStorage for Loan<'_> {
    fn metadata_uninit_mut(&mut self)->&mut[MaybeUninit<u8>] {&mut self.metadata}
    fn payload_uninit_mut(&mut self)->Option<&mut[MaybeUninit<u8>]> {self.payload.as_deref_mut()}
    fn record_header(&mut self,h:WrittenHeader)->Result<(),SendError> {
        if h.len!=self.metadata.len() {return Err(SendError::PayloadStorage);}
        self.probe.records.fetch_add(1,Order::Relaxed);Ok(())
    }
}
impl Drop for Loan<'_> {fn drop(&mut self){self.probe.returned.fetch_add(1,Order::Relaxed);}}
impl LendingRuntime for Probe {
    type Tx<'a>=Loan<'a>;type Rx=StaticFrame;
    fn loan(&self,_:BindingRoute,l:TxFrameLayout)->Result<Loan<'_>,LoanError>{
        self.loans.fetch_add(1,Order::Relaxed);
        Ok(Loan{metadata:Box::new_uninit_slice(l.metadata_len),payload:l.payload_len.map(Box::new_uninit_slice),probe:self})
    }
    fn publish(&self,_:BindingRoute,_:InitializedTx<Loan<'_>>)->Result<SendReport,SendError>{
        // Never inspect metadata even in a mutant implementation; no UB witness.
        self.published.fetch_add(1,Order::Relaxed);Ok(SendReport::exact(0,0))
    }
    fn receive(&self,_:BindingRoute)->Option<StaticFrame>{None}
    fn receive_bounded(&self,_:BindingRoute,_:&ReceiveBudget)->Option<StaticFrame>{None}
}
fn matrix(path:u8){
    // changing8->4/return8; matching short/zero answers; short/oversized returns;
    // callback refusal/unwind; valid full and valid all-zero extents.
    for (first,second,returned,action,success) in [
        (8,4,8,0,false),(8,4,4,0,false),(8,0,0,0,false),(8,8,0,0,false),
        (8,8,4,0,false),(8,8,9,0,false),(8,8,8,1,false),(8,8,8,2,false),
        (8,8,8,0,true),(0,0,0,0,true),
    ] {
        let region=Box::leak(Box::new(StartupRegion::<262144>::new()));let memory=region.claim().unwrap();
        let head=Box::leak(Box::new(Head{first,second,returned,action,calls:AtomicUsize::new(0)}));
        let envelopes=Box::leak(Box::new([Envelope::Custom(head)]));
        let setup=TransportSetup::handoff_on(&memory,&CLOCK,Setup{envelopes}).unwrap();
        let _publisher=Publisher::<u32>::on(&setup,&TestUri(1),0x8001).wire(TestWire{fail_encode:false}).build().unwrap();
        let route=setup.inner.borrow().tx.slice()[0];
        let _runtime=setup.enter_runtime().unwrap();head.calls.store(0,Order::Relaxed);
        let topic=TestUri(1).get_resource_uri(0x8001);
        let id=UUID::try_from_wire_pair(0x0000_0000_0000_7000,0x8000_0000_0000_0001).unwrap();
        let parts=TxHeaderParts{kind:UMessageType::Publish,priority:None,id:&id,reqid:None,ttl:None,comm_status:None,permission_level:None,source:&topic,sink:None,token:None,traceparent:None,payload_encoding:Some(crate::transport::tests::ENCODING),native_type_token:None};
        let probe=Probe::default();
        let result=catch_unwind(AssertUnwindSafe(||match path {
            0=>lend_value(&probe,route,ValueFrame{parts,wire:&route.admitted[0],size:EncodedSize::Exact(0),encode:&mut |_|Ok(()),reply:ReplyToken::default()}),
            1=>lend_encoded(&probe,route,EncodedFrame{parts,wire:Some(&route.admitted[0]),payload:PayloadBytes::one(&[]),reply:ReplyToken::default()}),
            _=> {
                let mut fill = |out: &mut [MaybeUninit<u8>]| { assert!(out.is_empty()); Ok(()) };
                // SAFETY: the agreed payload range is empty, so initialization
                // and byte-validity obligations are vacuous. The filler checks
                // that range and retains no borrow or ownership on any path.
                let frame = unsafe { InPlaceFrame::from_checked_fill(parts,&route.admitted[0],0,&mut fill) };
                lend_in_place(&probe,route,frame)
            },
        }));
        if action==2 {assert!(result.is_err());} else {assert_eq!(result.unwrap().is_ok(),success);}
        assert_eq!(probe.records.load(Order::Relaxed),usize::from(success));
        assert_eq!(probe.published.load(Order::Relaxed),usize::from(success));
        assert_eq!(probe.loans.load(Order::Relaxed),1);assert_eq!(probe.returned.load(Order::Relaxed),1);
    }
}
#[test]fn cpr1_value_lending_refuses_uninitialized_metadata_claims(){matrix(0);}
#[test]fn cpr1_encoded_lending_refuses_uninitialized_metadata_claims(){matrix(1);}
#[test]fn cpr1_in_place_lending_refuses_uninitialized_metadata_claims(){matrix(2);}

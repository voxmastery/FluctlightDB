#![cfg(feature = "distributed")]

use std::collections::BTreeSet;

use fluctlightdb::control::linearizability::{check_linearizable, TimedOperation};
use fluctlightdb::control::state_machine::ControlStateMachine;
use fluctlightdb::control::types::{
    ControlCommand, ControlResponse, NodeMetadata, TenantControlConfig,
};

#[derive(Clone)]
struct ObservedControl {
    command: ControlCommand,
    response: ControlResponse,
}

#[test]
fn model_checker_accepts_linearizable_control_history() {
    let register = ControlCommand::RegisterNode {
        request_id: "node-1".into(),
        node: NodeMetadata {
            node_id: 1,
            raft_addr: "127.0.0.1:1".into(),
            ..NodeMetadata::default()
        },
    };
    let create = ControlCommand::CreateTenant {
        tenant_id: "tenant-a".into(),
        request_id: "tenant-a".into(),
        config: TenantControlConfig::default(),
    };
    let voters = ControlCommand::SetVoters {
        request_id: "voters".into(),
        expected_membership_epoch: 0,
        voters: BTreeSet::from([1]),
    };
    let mut oracle = ControlStateMachine::new(&[9; 32]).unwrap();
    let observed = [register, create, voters].map(|command| {
        let response = oracle.apply(command.clone()).unwrap();
        ObservedControl { command, response }
    });
    let history = vec![
        TimedOperation::new(0, 2, observed[0].clone()),
        TimedOperation::new(1, 4, observed[1].clone()),
        TimedOperation::new(3, 5, observed[2].clone()),
    ];

    assert!(check_linearizable(
        ControlStateMachine::new(&[9; 32]).unwrap(),
        &history,
        |model, observed| model.apply(observed.command.clone()).unwrap() == observed.response
    ));
}

#[test]
fn model_checker_rejects_acknowledged_tenant_mutation_gap() {
    #[derive(Clone)]
    struct Ack {
        sequence: u64,
    }
    let history = vec![
        TimedOperation::new(0, 1, Ack { sequence: 1 }),
        TimedOperation::new(2, 3, Ack { sequence: 3 }),
    ];
    assert!(!check_linearizable(0_u64, &history, |watermark, ack| {
        if ack.sequence != *watermark + 1 {
            return false;
        }
        *watermark = ack.sequence;
        true
    }));
}

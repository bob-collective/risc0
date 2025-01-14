// Copyright 2024 RISC Zero, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

// Manages system calls for accelerators and other proof composition

use anyhow::Result;
use risc0_circuit_rv32im::prove::emu::addr::ByteAddr;
use risc0_zkvm_platform::{
    syscall::reg_abi::{REG_A3, REG_A4, REG_A5, REG_A6},
    WORD_SIZE,
};

use crate::{
    host::client::env::ProveKeccakRequest, recursion::prove::get_registered_zkr, Assumption,
    AssumptionReceipt,
};

use super::{Syscall, SyscallContext};

#[derive(Clone)]
pub(crate) struct SysProveKeccak;

impl Syscall for SysProveKeccak {
    fn syscall(
        &mut self,
        _syscall: &str,
        ctx: &mut dyn SyscallContext,
        _to_guest: &mut [u32],
    ) -> Result<(u32, u32)> {
        let po2 = ctx.load_register(REG_A3) as usize;
        let input_ptr = ByteAddr(ctx.load_register(REG_A4));
        let input_len = ctx.load_register(REG_A5);
        let input: Vec<u8> = ctx.load_region(input_ptr, input_len * WORD_SIZE as u32)?;
        let control_root = ctx.load_digest_from_register(REG_A6)?;

        let proof_request = ProveKeccakRequest { po2, input };

        let Some(keccak_coprocessor) = &ctx.syscall_table().keccak_coprocessor else {
            // Right now, we only support using a keccak coprocessor.
            unimplemented!()
        };

        let keccak_response = keccak_coprocessor
            .borrow_mut()
            .prove_keccak(proof_request)?;
        // Request for the ZKR to be lifted into the recursion circuit
        let zkr_proof_request = keccak_response.zkr_lift;
        let claim = zkr_proof_request.claim_digest;
        eprintln!("claim: {claim:?}");

        if let Some(coprocessor) = &ctx.syscall_table().coprocessor {
            coprocessor.borrow_mut().prove_zkr(zkr_proof_request)?;
        } else {
            get_registered_zkr(&zkr_proof_request.control_id)?;
            ctx.syscall_table()
                .pending_zkrs
                .borrow_mut()
                .push(zkr_proof_request);
        }

        let assumption = Assumption {
            claim,
            control_root,
        };
        ctx.syscall_table()
            .assumptions
            .borrow_mut()
            .0
            .push(AssumptionReceipt::Unresolved(assumption));

        Ok((0, 0))
    }
}

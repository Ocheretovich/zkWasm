1. Confirm you local 27107 mongodb is started.
2. Restore collection to db by:
`mongorestore -d zkwasm-mongo-merkle -c DATAHASH_0808080808080808080808080808080808080808080808080808080808080808 --dir=dump/zkwasm-mongo-merkle/DATAHASH_0808080808080808080808080808080808080808080808080808080808080808.bson`
`mongorestore -d zkwasm-mongo-merkle -c MERKLEDATA_0808080808080808080808080808080808080808080808080808080808080808 --dir=dump/zkwasm-mongo-merkle/MERKLEDATA_0808080808080808080808080808080808080808080808080808080808080808.bson`
3. run `run_test_prove.sh` and it will reproduce the issue.
4. If you remove/comment out the zkwasm-prover patch in Cargo.toml, it will work.
5. I had attached 2 log files: NoPatchOK.log and Patch23Fail.log for the log detail.

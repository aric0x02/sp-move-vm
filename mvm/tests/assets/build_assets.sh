#!/usr/bin/env bash

 function lockfile_waithold()
 {
    declare -ir time_beg=$(date '+%s')
    declare -ir time_max=7 # 7140 s = 1 hour 59 min.

    while ! \
       (set -o noclobber ; \
        echo -e "DATE:$(date)\nUSER:$(whoami)\nPID:$$" > /tmp/global.lock \
       ) 2>/dev/null
    do
        if [ $(($(date '+%s') - ${time_beg})) -gt ${time_max} ] ; then
            echo "Error: waited too long for lock file /tmp/global.lock" 1>&2
            return 1
        fi
        sleep 1
    done

    return 0
 }

 function lockfile_release()
 {
    rm -f /tmp/global.lock
 }

lockfile_waithold
set -e
dove build
dove call "store_u64(13)"
dove call "tx_test<0x01::Pontem::T>(100)"
dove deploy 
mv  ./build/assets/bundles/assets.pac ./build/assets/bundles/assets_old.pac
dove deploy valid_pack  --modules_exclude "ReflectTest"
mv  ./build/assets/bundles/assets.pac ./build/assets/bundles/valid_pack.pac
dove deploy invalid_pack --modules_exclude "Store" "ReflectTest"
mv  ./build/assets/bundles/assets.pac ./build/assets/bundles/invalid_pack.pac
mv  ./build/assets/bundles/assets_old.pac ./build/assets/bundles/assets.pac

dove call "rt_signers(rt)"
dove call "signers_tr_with_user(root)"
dove call "Assets::ScriptBook::test"
dove call "Assets::ScriptBook::test2(2,3)"
dove call "signer_order"

lockfile_release


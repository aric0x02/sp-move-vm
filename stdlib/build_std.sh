#!/usr/bin/env bash
 function lockfile_waithold()
 {
    declare -ir time_beg=$(date '+%s')
    declare -ir time_max=7140  # 7140 s = 1 hour 59 min.

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
rm -rf pont-stdlib
git clone https://github.com/pontem-network/pont-stdlib.git
cd pont-stdlib
git reset --hard 0702cdf5d696bc50b366e04de1b59ccc3d904032
dove build -b
cd ..

rm -rf move-stdlib
git clone https://github.com/pontem-network/move-stdlib.git
cd move-stdlib
git reset --hard ccd25dfc85c812f56b4a7120bce793edd5f19064
dove build -b
lockfile_release

# box.sh, sourced: box_binds fills BOX with bwrap's binds for a harness run or a package's script: this machine's
# programs and libraries and a few of /etc read-only, a passwd of its own on fd 3, new /dev, /proc and /tmp; no /run,
# /mnt or /home. The caller adds what it writes (its folder) and the program it runs, and opens fd 3 with box_passwd.
box_binds() {
  local x
  BOX=()
  for x in /usr /bin /sbin /lib /lib32 /lib64 /libx32; do  # merged /usr: a link stays a link
    if [ -L "$x" ]; then BOX+=(--symlink "$(readlink "$x")" "$x"); elif [ -d "$x" ]; then BOX+=(--ro-bind "$x" "$x"); fi
  done
  for x in ld.so.cache ld.so.conf ld.so.conf.d alternatives nsswitch.conf localtime ssl ca-certificates os-release terminfo; do
    [ -e "/etc/$x" ] && BOX+=(--ro-bind "/etc/$x" "/etc/$x")
  done
  BOX+=(--ro-bind-data 3 /etc/passwd --dev /dev --proc /proc --tmpfs /tmp --tmpfs /dev/shm)
}
box_passwd() {  # the one user the box knows: the caller's ids, its home the box's /tmp
  printf 'sandbox:x:%s:%s::/tmp:/bin/sh\n' "$(id -u)" "$(id -g)"
}

//! Rustified versions of openpty and forkpty based on BSD source implementations

/*-
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Copyright (c) 1990, 1993, 1994
 *	The Regents of the University of California.  All rights reserved.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 * 1. Redistributions of source code must retain the above copyright
 *    notice, this list of conditions and the following disclaimer.
 * 2. Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in the
 *    documentation and/or other materials provided with the distribution.
 * 3. Neither the name of the University nor the names of its contributors
 *    may be used to endorse or promote products derived from this software
 *    without specific prior written permission.
 *
 * THIS SOFTWARE IS PROVIDED BY THE REGENTS AND CONTRIBUTORS ``AS IS'' AND
 * ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
 * IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
 * ARE DISCLAIMED.  IN NO EVENT SHALL THE REGENTS OR CONTRIBUTORS BE LIABLE
 * FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
 * DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
 * OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
 * HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
 * LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
 * OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
 * SUCH DAMAGE.
 */


use libc::{c_char, c_int};

unsafe fn bail(fdm: c_int, fds: c_int) -> c_int {
    unsafe {
        let e =  *libc::_Errno();
        if fds >= 0 {
            libc::close(fds);
        }
        if fdm >= 0 {
            libc::close(fdm);
        }
        *libc::_Errno() = e;
    }
    -1
}

/// Rust-ified implementation of openpty based on free BSD source
pub unsafe fn openpty(
    amain: *mut c_int,
    asubord: *mut c_int,
    name: *mut c_char,
    termp: *const libc::termios,
    winp: *const libc::winsize,
) -> c_int {
    const PTC: &[u8] = b"/dev/ptc\0";

    unsafe {
        // Open the main pseudo-terminal device, making sure not to set it as the
        // controlling terminal for this process:
        let fdm = libc::open(PTC.as_ptr(),libc::O_RDWR | libc::O_NOCTTY);
        if fdm < 0 {
            return -1;
        }

        let fdm_path = libc::ttyname(fdm);
        if fdm_path.is_null() {
            return bail(fdm,-1);
        }

        // Set permissions and ownership on the subordinate device and unlock it:
        if libc::grantpt(fdm) < 0 || libc::unlockpt(fdm) < 0 {
            return bail(fdm, -1);
        }

        // Open the subordinate device without setting it as the controlling
        // terminal for this process:
        let fds = libc::open(fdm_path, libc::O_RDWR | libc::O_NOCTTY);
        if fds < 0 {
            return bail(fdm, -1);
        }

        let fds_path = libc::ttyname(fds);
        if fds_path.is_null() {
            return bail(fds, -1);
        }

        // If provided, set the terminal parameters:
        if !termp.is_null() && libc::tcsetattr(fdm, libc::TCSANOW, termp) != 0 {
            return bail(fdm, fds);
        }

        // If provided, set the window size:
        if !winp.is_null() && libc::ioctl(fds, libc::TIOCSWINSZ, winp) < 0 {
            return bail(fdm, fds);
        }

        // If the caller wants the name of the subordinate device, copy it out.
        //
        // Note that this is a terrible interface: there appears to be no standard
        // upper bound on the copy length for this pointer.  Nobody should pass
        // anything but NULL here, preferring instead to use ptsname(3C) directly.
        if !name.is_null() {
            libc::strcpy(name, fds_path);
        }

        *amain = fdm;
        *asubord = fds;
    }
    0
}

/// Rust-ified implementation of forkpty based on free BSD source
pub unsafe fn forkpty(
    amain: *mut c_int,
    name: *mut c_char,
    termp: *const libc::termios,
    winp: *const libc::winsize,
) -> libc::pid_t {
    const PTEM: &[u8] = b"ptem\0";
    const LDTERM: &[u8] = b"ldterm\0";
    let fds = -1;

    unsafe {
        let fdm = libc::posix_openpt(libc::O_RDWR);
        if fdm < 0 {
            return -1;
        }

        // Set permissions and ownership on the subordinate device and unlock it:
        if libc::grantpt(fdm) < 0 || libc::unlockpt(fdm) < 0 {
            return bail(fdm, -1);
        }

        *amain = fdm;

        let pid = libc::fork();
        if pid < 0 {
            return bail(*amain, fds);
        } else if pid > 0 {
            // In the parent process, we close the subordinate device and return the
            // process ID of the new child:
            libc::close(fds);
            return pid;
        }

        // The rest of this function executes in the child process.

        // Get the path name of the subordinate device:
        let subordpath = libc::ptsname(fdm);
        if subordpath.is_null() {
            return bail(fdm, -1);
        }

        // Open the subordinate device without setting it as the controlling
        // terminal for this process:
        let fds = libc::open(subordpath, libc::O_RDWR);
        if fds < 0 {
            return bail(fdm, -1);
        }

        // Check if the STREAMS modules are already pushed:
        let setup = libc::ioctl(fds, libc::I_FIND, LDTERM.as_ptr());
        if setup < 0 {
            return bail(fdm, fds);
        } else if setup == 0 {
            // The line discipline is not present, so push the appropriate STREAMS
            // modules for the subordinate device:
            if libc::ioctl(fds, libc::I_PUSH, PTEM.as_ptr()) < 0
                || libc::ioctl(fds, libc::I_PUSH, LDTERM.as_ptr()) < 0
            {
                return bail(fdm, fds);
            }
        }

        // If provided, set the terminal parameters:
        if !termp.is_null() && libc::tcsetattr(fds, libc::TCSAFLUSH, termp) != 0 {
            return bail(fdm, fds);
        }

        // If provided, set the window size:
        if !winp.is_null() && libc::ioctl(fds, libc::TIOCSWINSZ, winp) < 0 {
            return bail(fdm, fds);
        }

        // If the caller wants the name of the subordinate device, copy it out.
        //
        // Note that this is a terrible interface: there appears to be no standard
        // upper bound on the copy length for this pointer.  Nobody should pass
        // anything but NULL here, preferring instead to use ptsname(3C) directly.
        if !name.is_null() {
            libc::strcpy(name, subordpath);
        }

        // Close the main side of the pseudo-terminal pair:
        libc::close(*amain);

        // Use libc::TIOCSCTTY to set the subordinate device as our controlling
        // terminal.  This will fail (with ENOTTY) if we are not the leader in
        // our own session, so we call setsid() first.  Finally, arrange for
        // the pseudo-terminal to occupy the standard I/O descriptors.
        if libc::setsid() < 0
            // || libc::ioctl(fds, libc::TIOCSCTTY, 0) < 0
            || libc::dup2(fds, 0) < 0
            || libc::dup2(fds, 1) < 0
            || libc::dup2(fds, 2) < 0
        {
            // At this stage there are no particularly good ways to handle failure.
            // Exit as abruptly as possible, using _exit() to avoid messing with any
            // state still shared with the parent process.
            libc::_exit(libc::EXIT_FAILURE);
        }
        // Close the inherited descriptor, taking care to avoid closing the standard
        // descriptors by mistake:
        if fds > 2 {
            libc::close(fds);
        }
    }
    0
}

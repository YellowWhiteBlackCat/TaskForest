# TaskForest (TUI) binary-only RPM spec.
%global __os_install_post %{nil}
AutoReq: no
AutoProv: no

Name:       taskforest-t
Version:    %{version}
Release:    1%{?dist}
Summary:    Keyboard-centric terminal system monitor built with Ratatui
License:    Apache-2.0
URL:        https://github.com/YellowWhiteBlackCat/TaskForest
Source0:     taskforest-tree.tar.gz
Requires:   glibc
Suggests:   taskforest, smartmontools, nvme-cli, xfsprogs, iw
ExclusiveArch: x86_64 aarch64

%description
TaskForest renders live CPU, memory, GPU, network, storage, and per-process
telemetry through a high-performance, keyboard-driven terminal (TUI) frontend.
This package installs the TUI application (taskforest-t). Privileged telemetry
helpers are provided by the taskforest package or standalone helper installations.

No display server or graphical libraries are required: runs anywhere in standard
terminal environments, SSH sessions, and headless servers.

%prep

%build

%install
mkdir -p %{buildroot}
tar -xf %{SOURCE0} -C %{buildroot}

%files
/usr/bin/taskforest-t
/usr/bin/taskmanager-tui
%dir /usr/share/licenses/taskforest-t
/usr/share/licenses/taskforest-t/LICENSE
/usr/share/licenses/taskforest-t/THIRD-PARTY-NOTICES.txt
%dir /usr/share/doc/taskforest-t
/usr/share/doc/taskforest-t/README.md
/usr/share/doc/taskforest-t/copyright

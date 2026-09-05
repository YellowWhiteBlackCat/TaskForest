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
Recommends: polkit
Suggests:   smartmontools, nvme-cli, xfsprogs, iw
ExclusiveArch: x86_64 aarch64

%description
TaskForest renders live CPU, memory, GPU, network, storage, and per-process
telemetry through a high-performance, keyboard-driven terminal (TUI) frontend.
This package installs the TUI application and the per-feature polkit-gated helpers (ADR-023).

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
/usr/libexec/taskforest-setup-helper
/usr/libexec/taskforest-privilege-helper
/usr/libexec/taskforest-net-launcher
/usr/libexec/taskforest-process-control-helper
/usr/libexec/taskforest-smbios-helper
/usr/libexec/taskforest-rapl-helper
/usr/libexec/taskforest-msr-helper
%dir /usr/share/taskforest
%dir /usr/share/taskforest/setup
/usr/share/taskforest/setup/99-taskforest.rules
/usr/share/polkit-1/actions/io.github.YellowWhiteBlackCat.TaskForest.perf-helper.policy
/usr/share/polkit-1/actions/io.github.YellowWhiteBlackCat.TaskForest.net-launcher.policy
/usr/share/polkit-1/actions/io.github.YellowWhiteBlackCat.TaskForest.process-control.policy
/usr/share/polkit-1/actions/io.github.YellowWhiteBlackCat.TaskForest.smbios-helper.policy
/usr/share/polkit-1/actions/io.github.YellowWhiteBlackCat.TaskForest.rapl-helper.policy
/usr/share/polkit-1/actions/io.github.YellowWhiteBlackCat.TaskForest.msr-helper.policy
/usr/share/polkit-1/actions/io.github.YellowWhiteBlackCat.TaskForest.setup.policy
%dir /usr/share/licenses/taskforest
/usr/share/licenses/taskforest/LICENSE
/usr/share/licenses/taskforest/THIRD-PARTY-NOTICES.txt

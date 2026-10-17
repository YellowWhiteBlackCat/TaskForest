# taskforest-daemon service RPM spec.
%global __os_install_post %{nil}
AutoReq: no
AutoProv: no

Name:       taskforest-daemon
Version:    %{version}
Release:    1%{?dist}
Summary:    Background telemetry and persistence daemon for TaskForest
License:    Apache-2.0
URL:        https://github.com/YellowWhiteBlackCat/TaskForest

Requires:   polkit

%description
TaskForest daemon runs as a continuous system service (systemd) providing
7x24 background telemetry persistence and IPC metric streaming for TaskForest
frontends (GPUI, Iced, TUI, Bevy).

%files
%defattr(-,root,root,-)
/usr/lib/systemd/system/taskforest-daemon.service
/usr/lib/systemd/user/taskforest-daemon.service

# TaskForest binary-only RPM spec.
#
# The %files list mirrors the staged /usr tree from
# packaging/linux/stage-release-tree.sh (the PKGBUILD layout authority). The
# staged tree is shipped as Source0 and unpacked straight into the build root,
# so this spec owns metadata only — never a second copy of the layout.
#
# BuildRequires are deliberately empty: the binaries are prebuilt by the
# release pipeline, and the brp scripts are disabled so a Fedora-destined
# package never inherits Ubuntu-host dependency discovery.
%global __os_install_post %{nil}
AutoReq: no
AutoProv: no

Name:       taskforest
Version:    %{version}
Release:    1%{?dist}
Summary:    Eye-friendly native system monitor built with GPUI
License:    Apache-2.0
URL:        https://github.com/YellowWhiteBlackCat/TaskForest
Source0:     taskforest-tree.tar.gz
Requires:   fontconfig, freetype, libxkbcommon, libwayland-client, vulkan-loader
Recommends: polkit
Suggests:   smartmontools, nvme-cli, xfsprogs, iw, mesa-vulkan-drivers
ExclusiveArch: x86_64 aarch64

%description
TaskForest renders live CPU, memory, GPU, network, storage, and per-process
telemetry through one Wayland-native GPUI frontend. This package installs the
GPUI application and the per-feature polkit-gated helpers (ADR-023): nothing
runs privileged until an OS-native prompt authorizes it. History is persisted
by the active frontend session.

A Wayland session is required (X11 is not supported). The Vulkan loader plus a
Vulkan ICD for your GPU are needed for rendering.

%prep

%build

%install
mkdir -p %{buildroot}
tar -xf %{SOURCE0} -C %{buildroot}

%files
/usr/bin/taskmanager
/usr/bin/taskforest-g
/usr/libexec/taskforest-setup-helper
/usr/libexec/taskforest-privilege-helper
/usr/libexec/taskforest-net-launcher
/usr/libexec/taskforest-process-control-helper
/usr/libexec/taskforest-smbios-helper
/usr/libexec/taskforest-rapl-helper
/usr/libexec/taskforest-msr-helper
/usr/share/applications/io.github.YellowWhiteBlackCat.TaskForestG.desktop
/usr/share/metainfo/io.github.YellowWhiteBlackCat.TaskForestG.metainfo.xml
/usr/share/icons/hicolor/scalable/apps/taskforest-taskboard.svg
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

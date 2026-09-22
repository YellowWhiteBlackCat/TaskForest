# TaskForest (Iced) binary-only RPM spec.
%global __os_install_post %{nil}
AutoReq: no
AutoProv: no

Name:       taskforest-i
Version:    %{version}
Release:    1%{?dist}
Summary:    Eye-friendly native system monitor built with Iced
License:    Apache-2.0
URL:        https://github.com/YellowWhiteBlackCat/TaskForest
Source0:     taskforest-tree.tar.gz
Requires:   taskforest-common, fontconfig, freetype, libxkbcommon, libwayland-client, vulkan-loader
Recommends: polkit
Suggests:   taskforest, smartmontools, nvme-cli, xfsprogs, iw, mesa-vulkan-drivers
ExclusiveArch: x86_64 aarch64

%description
TaskForest renders live CPU, memory, GPU, network, storage, and per-process
telemetry through one Wayland-native Iced frontend. This package installs the
Iced application. Privileged telemetry helpers are provided by the taskforest
package or standalone helper installations.

A Wayland session is required (X11 is not supported). The Vulkan loader plus a
Vulkan ICD for your GPU are needed for rendering.

%prep

%build

%install
mkdir -p %{buildroot}
tar -xf %{SOURCE0} -C %{buildroot}

%files
/usr/bin/taskforest-i
/usr/share/applications/io.github.YellowWhiteBlackCat.TaskForestI.desktop
/usr/share/metainfo/io.github.YellowWhiteBlackCat.TaskForestI.metainfo.xml
%dir /usr/share/licenses/taskforest-i
/usr/share/licenses/taskforest-i/LICENSE
/usr/share/licenses/taskforest-i/THIRD-PARTY-NOTICES.txt

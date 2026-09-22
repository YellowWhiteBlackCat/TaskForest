# TaskForest (Bevy UI) binary-only RPM spec.
%global __os_install_post %{nil}
AutoReq: no
AutoProv: no

Name:       taskforest-b
Version:    %{version}
Release:    1%{?dist}
Summary:    Native system monitor built with Bevy UI
License:    Apache-2.0
URL:        https://github.com/YellowWhiteBlackCat/TaskForest
Source0:     taskforest-tree.tar.gz
Requires:   taskforest-common, fontconfig, freetype, libxkbcommon, libwayland-client, vulkan-loader
Recommends: polkit
Suggests:   taskforest, smartmontools, nvme-cli, xfsprogs, iw, mesa-vulkan-drivers
ExclusiveArch: x86_64 aarch64

%description
TaskForest renders live CPU, memory, GPU, network, storage, and per-process
telemetry through a Wayland-native Bevy UI frontend. This package installs the
Bevy UI application. Privileged telemetry helpers are provided by the taskforest
package or standalone helper installations.

A Wayland session is required (X11 is not supported). The Vulkan loader plus a
Vulkan ICD for your GPU are needed for rendering.

%prep

%build

%install
mkdir -p %{buildroot}
tar -xf %{SOURCE0} -C %{buildroot}

%files
/usr/bin/taskforest-b
/usr/share/applications/io.github.YellowWhiteBlackCat.TaskForestB.desktop
/usr/share/metainfo/io.github.YellowWhiteBlackCat.TaskForestB.metainfo.xml
%dir /usr/share/licenses/taskforest-b
/usr/share/licenses/taskforest-b/LICENSE
/usr/share/licenses/taskforest-b/THIRD-PARTY-NOTICES.txt

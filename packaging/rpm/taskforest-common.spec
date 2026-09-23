# taskforest-common binary-only RPM spec.
#
# The data-only package owns the assets shared by the four frontend products
# (the hicolor icon set). Exactly one RPM owns each shared path; the frontend
# specs Require this package and no longer list the icons
# (docs/SYSTEM_INSTALL_MANIFEST.md). The monolithic Arch package still ships
# its own icons through packaging/arch/PKGBUILD.
#
# BuildRequires are deliberately empty: the assets are repo content copied into
# the staged tree, and the brp scripts are disabled so a Fedora-destined
# package never inherits Ubuntu-host dependency discovery.
%global __os_install_post %{nil}
AutoReq: no
AutoProv: no

Name:       taskforest-common
Version:    %{version}
Release:    1%{?dist}
Summary:    Shared data assets for the TaskForest frontends
License:    Apache-2.0
URL:        https://github.com/YellowWhiteBlackCat/TaskForest
Source0:    taskforest-tree.tar.gz
# The shared hicolor icon set moved out of the frontend RPMs into this data
# package at the first split release, held once in packaging/split-version
# (shared with control-common). A single `dnf upgrade` transaction moves the
# file cleanly because RPM ignores a conflict whose old owner is upgraded in
# the same transaction, but a host that upgrades one frontend without the
# others would otherwise hit an implicit file conflict. The versioned conflict
# makes the resolver upgrade the old owner instead; `Obsoletes` is deliberately
# absent because it would erase the frontend product rather than upgrade it.
# The boundary is a historical constant naming the first release that ships
# this package; it must not float with the current version. build-rpm.sh
# renders it through the same `-`→`~` transform as the package version, because
# RPM orders `~` below the final release: a plain `< 0.2.0` would also match the
# prerelease `0.2.0~rc1` and conflict with its own frontends. Fedora
# Packaging:Conflicts, "Splitting Packages".
#
# Only `taskforest` (GPUI) is named. It is the one RPM ever published before
# the split (v0.1.3, whose %files carried the shared hicolor path), so it is
# the only RPM owner this conflict can meet. `taskforest-i`/`taskforest-b`
# shipped as DEBs at <=0.1.3 and are covered by control-common's `Replaces`/
# `Breaks`, but their RPM specs first landed after v0.1.3 (the never-released
# 0.1.4 tree), no `taskforest-i`/`taskforest-b` RPM was ever published, and
# their split-era RPMs strip the common destinations (build-rpm.sh). No
# reachable RPM upgrade path carries the shared icon under those names, so
# naming them would guard a package that cannot exist.
Conflicts:  taskforest < %{split_version}
ExclusiveArch: x86_64 aarch64

%description
TaskForest ships four independent frontend products (TaskForest-G,
TaskForest-I, TaskForest-T, and TaskForest-B) over one product identity. This
data-only package owns the assets those products share — the hicolor
application icon ladder — so exactly one package owns each shared path instead
of every frontend installing its own byte-identical copy.

It installs no executable, helper, or privileged asset.

%prep

%build

%install
mkdir -p %{buildroot}
tar -xf %{SOURCE0} -C %{buildroot}

%files
/usr/share/icons/hicolor/scalable/apps/taskforest-taskboard.svg
/usr/share/icons/hicolor/16x16/apps/taskforest-taskboard.png
/usr/share/icons/hicolor/24x24/apps/taskforest-taskboard.png
/usr/share/icons/hicolor/32x32/apps/taskforest-taskboard.png
/usr/share/icons/hicolor/48x48/apps/taskforest-taskboard.png
/usr/share/icons/hicolor/64x64/apps/taskforest-taskboard.png
/usr/share/icons/hicolor/128x128/apps/taskforest-taskboard.png
/usr/share/icons/hicolor/256x256/apps/taskforest-taskboard.png
/usr/share/icons/hicolor/512x512/apps/taskforest-taskboard.png

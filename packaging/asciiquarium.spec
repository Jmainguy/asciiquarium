Name:           asciiquarium
Version:        %{?version}%{!?version:2.0.0}
Release:        1%{?dist}
Summary:        Colorful aquarium animation for your terminal
License:        GPL-2.0-or-later
URL:            https://github.com/Jmainguy/asciiquarium
Vendor:         Jmainguy
Packager:       Jonathan Seth Mainguy <jon@soh.re>

%description
Asciiquarium is a self-contained Rust terminal animation inspired by Kirk
Baucom's original Asciiquarium. It includes the classic fish, surface and sea
creatures, bubbles, seaweed, waves, castle, collision behavior, terminal
resizing, and keyboard controls without a Perl runtime.

%prep

%build

%install
install -Dpm0755 %{_sourcedir}/asciiquarium %{buildroot}%{_bindir}/asciiquarium
install -Dpm0644 %{_sourcedir}/gpl.txt %{buildroot}%{_licensedir}/%{name}/gpl.txt

%files
%license %{_licensedir}/%{name}/gpl.txt
%{_bindir}/asciiquarium

%changelog
* Sun Aug 23 2026 Jonathan Seth Mainguy <jon@soh.re> - 2.0.0-1
- Rewrite asciiquarium in Rust

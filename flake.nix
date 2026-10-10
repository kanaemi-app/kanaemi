{
  description = "Kanaemi, a cross-platform Japanese input method";

  inputs = {
    nixpkgs.url = "https://channels.nixos.org/nixos-unstable/nixexprs.tar.xz";
    rust-overlay = {
      url = "git+https://github.com/oxalica/rust-overlay?shallow=1";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane.url = "github:ipetkov/crane";
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
      crane,
      ...
    }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f system);
      pkgsFor =
        system:
        import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };
    in
    {
      packages = forAllSystems (
        system:
        let
          pkgs = pkgsFor system;
          inherit (pkgs) lib;
          inherit (pkgs.stdenv.hostPlatform) isDarwin;
          rust = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
          craneLib = (crane.mkLib pkgs).overrideToolchain (_: rust);
          workspace = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).workspace.package;
          # The source carries no tags, so a build names the commit it is made from.
          version = "${workspace.version}-${self.shortRev or self.dirtyShortRev or "unknown"}";
          build = {
            pname = "kanaemi";
            inherit version;
            src = lib.fileset.toSource {
              root = ./.;
              fileset = lib.fileset.unions [
                ./Cargo.toml
                ./Cargo.lock
                ./apps
                ./crates
              ];
            };
            strictDeps = true;
            KANAEMI_BUILD_VERSION = version;
            # The tests run in CI and with `just test`.
            doCheck = false;
          }
          // (
            if isDarwin then
              {
                cargoExtraArgs = "-p kanaemi-macos -p kanaemi-settings";
                buildInputs = [ pkgs.libiconv ];
              }
            else
              {
                cargoExtraArgs = "-p kanaemi-ibus -p kanaemi-settings";
                nativeBuildInputs = [ pkgs.pkg-config ];
                # What the Dioxus desktop renderer (wry) links against.
                buildInputs = [
                  pkgs.webkitgtk_4_1
                  pkgs.gtk3
                  pkgs.libsoup_3
                  pkgs.glib
                  pkgs.openssl
                  pkgs.xdotool
                ];
              }
          );
          install =
            if isDarwin then
              {
                # What apps/macos/bundle.sh runs.
                nativeBuildInputs = [
                  pkgs.librsvg
                  pkgs.perl
                ];
                # iconutil and codesign come with macOS, outside the store; Nix
                # builds on macOS reach them, as they run without a sandbox by
                # default. The codesign in the store signs a binary but does not
                # seal a bundle.
                installPhaseCommand = ''
                  mkdir -p macos-tools
                  ln -s /usr/bin/codesign /usr/bin/iconutil macos-tools/
                  PATH=$PWD/macos-tools:$PATH bash apps/macos/bundle.sh target/release
                  mkdir -p $out/Applications
                  cp -R target/bundle.noindex/Kanaemi.app $out/Applications/
                '';
                # Stripping would break the bundle's signature.
                dontStrip = true;
                # Removing the references to the vendored sources rewrites the
                # binaries after the install and signs them alone, so the bundle
                # is sealed again.
                postFixup = ''
                  app=$out/Applications/Kanaemi.app
                  /usr/bin/codesign --force --sign - $app/Contents/Resources/KanaemiSettings.app
                  /usr/bin/codesign --force --sign - $app
                '';
              }
            else
              {
                nativeBuildInputs = build.nativeBuildInputs ++ [ pkgs.wrapGAppsHook3 ];
                installPhaseCommand = ''
                  lib=$out/lib/kanaemi
                  install -Dm755 target/release/kanaemi-ibus $lib/kanaemi-ibus
                  # The engine opens the settings app from beside itself.
                  install -Dm755 target/release/kanaemi-settings $lib/kanaemi-settings
                  install -Dm644 apps/macos/assets/logo/kanaemi-icon.svg $lib/kanaemi.svg
                  # Where NixOS finds the engines given to i18n.inputMethod.ibus.engines.
                  mkdir -p $out/share/ibus/component
                  substitute apps/ibus/kanaemi.xml $out/share/ibus/component/kanaemi.xml \
                    --replace-fail @LIBDIR@ $lib --replace-fail @VERSION@ ${version}
                '';
                # The settings app is not in bin, where the hook looks.
                dontWrapGApps = true;
                postFixup = ''
                  wrapGApp $out/lib/kanaemi/kanaemi-settings
                '';
              };
          kanaemi = craneLib.buildPackage (
            build
            // install
            // {
              cargoArtifacts = craneLib.buildDepsOnly build;
              # The install above takes what it needs from target/release.
              doNotPostBuildInstallCargoBinaries = true;
            }
          );
        in
        {
          inherit kanaemi;
          default = kanaemi;
        }
      );

      devShells = forAllSystems (
        system:
        let
          pkgs = pkgsFor system;
          rust = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        in
        {
          default = pkgs.mkShell {
            packages = [
              rust
              pkgs.just
            ]
            ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isDarwin [
              pkgs.libiconv
              # Renders the input source icon from the logo SVG (apps/macos/install.sh).
              pkgs.librsvg
            ]
            # What the Dioxus desktop renderer (wry) links against on Linux.
            ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux [
              pkgs.pkg-config
              pkgs.webkitgtk_4_1
              pkgs.gtk3
              pkgs.libsoup_3
              pkgs.glib
              pkgs.openssl
              pkgs.xdotool
              # What the Fcitx5 add-on's C++ layer builds against.
              pkgs.fcitx5
            ];
          };
        }
      );

      formatter = forAllSystems (system: nixpkgs.legacyPackages.${system}.nixfmt);
    };
}

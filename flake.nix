{
  description = "FCM to UnifiedPush relay framework";

  outputs =
    { self, ... }@args:
    let
      inputs = (import ./.tack) { overrides = args.tackOverrides or { }; };
      inherit (inputs) fenix nixpkgs;
      inherit (nixpkgs) lib;

      systems = lib.systems.doubles.linux ++ lib.systems.doubles.darwin;
      forAllSystems = lib.genAttrs systems;
      pkgsFor = system: nixpkgs.legacyPackages.${system} or (import nixpkgs { inherit system; });
      androidPkgsFor =
        system:
        import nixpkgs {
          inherit system;
          config = {
            allowUnfree = true;
            android_sdk.accept_license = true;
          };
        };
      hasFenix = system: fenix.packages ? ${system};
      rustToolchainFor =
        system:
        let
          pkgs = pkgsFor system;
        in
        if hasFenix system then
          with fenix.packages.${system};
          combine [
            latest.cargo
            latest.clippy
            latest.rust-src
            latest.rustc
            latest.rustfmt
            targets.aarch64-linux-android.latest.rust-std
          ]
        else
          pkgs.rustc;
      rustPlatformFor =
        system:
        let
          pkgs = pkgsFor system;
          toolchain = rustToolchainFor system;
        in
        if hasFenix system then
          pkgs.makeRustPlatform {
            cargo = toolchain;
            rustc = toolchain;
          }
        else
          pkgs.rustPlatform;
      toolchainFor =
        system:
        let
          pkgs = pkgsFor system;
        in
        if hasFenix system then
          [ (rustToolchainFor system) ]
        else
          (with pkgs; [
            cargo
            clippy
            rustc
            rustfmt
          ]);

      perSystem =
        system:
        let
          pkgs = androidPkgsFor system;
          android = import ./nix/android.nix { inherit pkgs; };
        in
        {
          packages = import ./nix/packages.nix {
            inherit android lib pkgs;
            rustPlatform = rustPlatformFor system;
          };
          devShell = import ./nix/dev-shell.nix {
            inherit android pkgs;
            toolchain = toolchainFor system;
          };
          apps = import ./nix/apps.nix {
            inherit
              android
              pkgs
              self
              system
              ;
          };
        };
    in
    {
      packages = forAllSystems (system: (perSystem system).packages);
      devShells = forAllSystems (system: {
        default = (perSystem system).devShell;
      });
      apps = forAllSystems (system: (perSystem system).apps);

      nixosModules.default = import ./nix/module.nix self;

      overlays.default =
        _final: prev:
        let
          packages = self.packages.${prev.stdenv.hostPlatform.system};
        in
        {
          inherit (packages)
            pushcompat-bridge
            pushcompat-patcher
            pushcompat-shim
            ;
        };

      formatter = forAllSystems (system: (pkgsFor system).nixfmt);
    };
}

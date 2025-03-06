{
  description = "EmmyLua Language Server";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      nixpkgs,
      rust-overlay,
      flake-utils,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };

        inherit (pkgs) rustPlatform;
      in
      {
        packages =
          let
            mkPackage =
              x:
              let
                cargoToml = builtins.fromTOML (builtins.readFile ./crates/${x}/Cargo.toml);
              in
              rustPlatform.buildRustPackage {
                pname = cargoToml.package.name;
                version = cargoToml.package.version;

                src = ./.;
                cargoLock.lockFile = ./Cargo.lock;

                nativeBuildInputs = with pkgs; [ pkg-config ];
                buildInputs = with pkgs; [ stdenv.cc.cc.lib ];

                buildAndTestSubdir = "crates/${x}";

                postFixup = ''
                  patchelf --set-rpath "${pkgs.stdenv.cc.cc.lib}/lib" $out/bin/${x}
                '';
              };
          in
          rec {
            emmylua_ls = mkPackage "emmylua_ls";
            emmylua_doc_cli = mkPackage "emmylua_doc_cli";
            emmylua_check = mkPackage "emmylua_check";
            default = emmylua_ls;
          };
      }
    );
}

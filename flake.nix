{
  description = "A very basic flake";

  inputs = {
    nixpkgs.url = "https://channels.nixos.org/nixpkgs-unstable/nixexprs.tar.zst";
  };

  outputs =
    { nixpkgs, ... }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
    in
    {
      devShells.${system}.default = pkgs.mkShell {
        packages = with pkgs; [
          # Rust
          cargo
          rustc
          rust-analyzer
          rustfmt

        ];

        # env.RUSTFLAGS = "-C link-args=-Wl,-rpath,${
        #   pkgs.lib.makeLibraryPath [
        #     pkgs.wayland
        #     pkgs.vulkan-loader
        #     pkgs.libxkbcommon
        #   ]
        # }";
      };
    };
}

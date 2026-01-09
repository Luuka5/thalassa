{
  description = "Thalassa";


  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        rustToolchain = pkgs.rust-bin.stable.latest.default;

        thalassa = pkgs.rustPlatform.buildRustPackage {
          pname = "thalassa";
          version = "0.1.0";

          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;

            # Git dependencies require fixed output hashes for vendoring.
            # Fill this with the hash Nix prints when the build fails.
            outputHashes = {
              "mothership-0.1.0" = "sha256-HS8mJGWkMDo61tAZjndAaa1CYtKafUPZlczHNDPg9VU=";
            };
          };

          nativeBuildInputs = with pkgs; [
            rustToolchain
            makeWrapper
            installShellFiles
            pkg-config
          ];

          # NOTE: thalassa depends on mothership via a git dependency.
          # For reproducible builds you should set `cargoHash`.
          # If you prefer easy iteration (always updateable deps), you can use:
          #   nix build --impure --option sandbox false
          # and remove cargoHash. (Not reproducible.)

          buildInputs = with pkgs; [
            # Add link-time deps here if needed
          ];

          # Keep runtime tools reachable if the binary shells out.
          postInstall = ''
            # If thalassa uses docker or other CLIs at runtime, add them here.
            wrapProgram $out/bin/thalassa \
              --prefix PATH : ${pkgs.lib.makeBinPath [ pkgs.docker pkgs.docker-compose ]}

            # Completions if the binary supports: `thalassa completion <shell>`
            # installShellCompletion --cmd thalassa \
            #   --bash <($out/bin/thalassa completion bash) \
            #   --fish <($out/bin/thalassa completion fish) \
            #   --zsh <($out/bin/thalassa completion zsh)
          '';

          meta = with pkgs.lib; {
            description = "Thalassa service";
            license = licenses.mit;
            maintainers = [ ];
          };
        };
      in
      {
        packages = {
          default = thalassa;
          thalassa = thalassa;
        };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustToolchain
            rust-analyzer
            cargo
            rustc
            git
            docker
            docker-compose
          ];

          shellHook = ''
            echo "Thalassa development environment"
            echo "Rust version: $(rustc --version)"
          '';
        };

        apps.default = {
          type = "app";
          program = "${thalassa}/bin/thalassa";
        };
      }
    ) // {
      overlays.default = final: prev: {
        thalassa = self.packages.${final.system}.default;
      };
    };
}

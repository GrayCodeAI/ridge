class Ridge < Formula
  desc "Mission control for your AI agents. Terminal-native TUI."
  homepage "https://github.com/GrayCodeAI/ridge"
  license "MIT"
  head "https://github.com/GrayCodeAI/ridge.git", branch: "main"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "ridge", shell_output("#{bin}/ridge --version")
  end
end

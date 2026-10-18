class Term < Formula
  desc "Mission control for your AI agents. Terminal-native TUI."
  homepage "https://gryacodeai.com/term"
  url "https://github.com/Patel230/term/archive/refs/tags/v0.1.0.tar.gz"
  sha256 "PLACEHOLDER_SHA256"
  license "MIT"
  head "https://github.com/Patel230/term.git", branch: "main"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "term", shell_output("#{bin}/term --version")
  end
end
EOF
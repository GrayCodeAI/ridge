class Term < Formula
  desc "Mission control for your AI agents. Terminal-native TUI."
  homepage "https://gryacodeai.com/term"
  url "https://github.com/GrayCodeAI/term/archive/refs/tags/v0.1.0.tar.gz"
  sha256 "3ac9fed1033461acaedf9ace6ec9718dceed05a300fd3f5d4806ae437b336be6"
  license "MIT"
  head "https://github.com/GrayCodeAI/term.git", branch: "main"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "term", shell_output("#{bin}/term --version")
  end
end
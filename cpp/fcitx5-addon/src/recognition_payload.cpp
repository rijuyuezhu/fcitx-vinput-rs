#include "vinput_fcitx_bridge/recognition_payload.h"

#include <cctype>
#include <cstdint>
#include <optional>
#include <string_view>
#include <utility>

namespace vinput_fcitx_bridge {
namespace {

class JsonCursor {
public:
  explicit JsonCursor(std::string_view input) : input_(input) {}

  RecognitionPayload parsePayload() {
    RecognitionPayload payload;
    skipSpace();
    if (!consume('{')) {
      return payload;
    }

    while (true) {
      skipSpace();
      if (consume('}')) {
        normalize(&payload);
        return payload;
      }

      auto key = parseString();
      if (!key) {
        return {};
      }
      skipSpace();
      if (!consume(':')) {
        return {};
      }

      if (*key == "commit_text") {
        auto value = parseString();
        if (!value) {
          return {};
        }
        payload.commit_text = std::move(*value);
      } else if (*key == "candidates") {
        if (!parseCandidates(&payload.candidates)) {
          return {};
        }
      } else if (!skipValue()) {
        return {};
      }

      skipSpace();
      if (consume('}')) {
        normalize(&payload);
        return payload;
      }
      if (!consume(',')) {
        return {};
      }
    }
  }

private:
  void skipSpace() {
    while (pos_ < input_.size() &&
           std::isspace(static_cast<unsigned char>(input_[pos_])) != 0) {
      ++pos_;
    }
  }

  bool consume(char expected) {
    skipSpace();
    if (pos_ >= input_.size() || input_[pos_] != expected) {
      return false;
    }
    ++pos_;
    return true;
  }

  std::optional<std::string> parseString() {
    skipSpace();
    if (pos_ >= input_.size() || input_[pos_] != '"') {
      return std::nullopt;
    }
    ++pos_;

    std::string output;
    while (pos_ < input_.size()) {
      const unsigned char ch = static_cast<unsigned char>(input_[pos_++]);
      if (ch == '"') {
        return output;
      }
      if (ch != '\\') {
        output.push_back(static_cast<char>(ch));
        continue;
      }
      if (pos_ >= input_.size()) {
        return std::nullopt;
      }
      const char escape = input_[pos_++];
      switch (escape) {
      case '"':
      case '\\':
      case '/':
        output.push_back(escape);
        break;
      case 'b':
        output.push_back('\b');
        break;
      case 'f':
        output.push_back('\f');
        break;
      case 'n':
        output.push_back('\n');
        break;
      case 'r':
        output.push_back('\r');
        break;
      case 't':
        output.push_back('\t');
        break;
      case 'u': {
        auto codepoint = parseUnicodeEscape();
        if (!codepoint) {
          return std::nullopt;
        }
        appendUtf8(*codepoint, &output);
        break;
      }
      default:
        return std::nullopt;
      }
    }
    return std::nullopt;
  }

  std::optional<char32_t> parseUnicodeEscape() {
    auto high = parseHexQuad();
    if (!high) {
      return std::nullopt;
    }

    if (*high < 0xD800 || *high > 0xDBFF) {
      return high;
    }

    if (pos_ + 1 >= input_.size() || input_[pos_] != '\\' || input_[pos_ + 1] != 'u') {
      return std::nullopt;
    }
    pos_ += 2;
    auto low = parseHexQuad();
    if (!low || *low < 0xDC00 || *low > 0xDFFF) {
      return std::nullopt;
    }

    return 0x10000 + ((*high - 0xD800) << 10) + (*low - 0xDC00);
  }

  std::optional<char32_t> parseHexQuad() {
    if (pos_ + 4 > input_.size()) {
      return std::nullopt;
    }
    char32_t value = 0;
    for (int i = 0; i < 4; ++i) {
      const char ch = input_[pos_++];
      value <<= 4;
      if (ch >= '0' && ch <= '9') {
        value += static_cast<char32_t>(ch - '0');
      } else if (ch >= 'a' && ch <= 'f') {
        value += static_cast<char32_t>(ch - 'a' + 10);
      } else if (ch >= 'A' && ch <= 'F') {
        value += static_cast<char32_t>(ch - 'A' + 10);
      } else {
        return std::nullopt;
      }
    }
    return value;
  }

  static void appendUtf8(char32_t codepoint, std::string *output) {
    if (codepoint <= 0x7F) {
      output->push_back(static_cast<char>(codepoint));
    } else if (codepoint <= 0x7FF) {
      output->push_back(static_cast<char>(0xC0 | (codepoint >> 6)));
      output->push_back(static_cast<char>(0x80 | (codepoint & 0x3F)));
    } else if (codepoint <= 0xFFFF) {
      output->push_back(static_cast<char>(0xE0 | (codepoint >> 12)));
      output->push_back(static_cast<char>(0x80 | ((codepoint >> 6) & 0x3F)));
      output->push_back(static_cast<char>(0x80 | (codepoint & 0x3F)));
    } else {
      output->push_back(static_cast<char>(0xF0 | (codepoint >> 18)));
      output->push_back(static_cast<char>(0x80 | ((codepoint >> 12) & 0x3F)));
      output->push_back(static_cast<char>(0x80 | ((codepoint >> 6) & 0x3F)));
      output->push_back(static_cast<char>(0x80 | (codepoint & 0x3F)));
    }
  }

  bool parseCandidates(std::vector<Candidate> *candidates) {
    if (!consume('[')) {
      return false;
    }
    skipSpace();
    if (consume(']')) {
      return true;
    }

    while (true) {
      auto candidate = parseCandidate();
      if (!candidate) {
        return false;
      }
      if (!candidate->text.empty() || candidate->source == CandidateSource::Cancel) {
        candidates->push_back(std::move(*candidate));
      }

      skipSpace();
      if (consume(']')) {
        return true;
      }
      if (!consume(',')) {
        return false;
      }
    }
  }

  std::optional<Candidate> parseCandidate() {
    Candidate candidate;
    if (!consume('{')) {
      return std::nullopt;
    }

    while (true) {
      skipSpace();
      if (consume('}')) {
        return candidate;
      }

      auto key = parseString();
      if (!key) {
        return std::nullopt;
      }
      skipSpace();
      if (!consume(':')) {
        return std::nullopt;
      }

      if (*key == "text") {
        auto value = parseString();
        if (!value) {
          return std::nullopt;
        }
        candidate.text = std::move(*value);
      } else if (*key == "source") {
        auto value = parseString();
        if (!value) {
          return std::nullopt;
        }
        candidate.source = CandidateSourceFromWire(*value);
      } else if (!skipValue()) {
        return std::nullopt;
      }

      skipSpace();
      if (consume('}')) {
        return candidate;
      }
      if (!consume(',')) {
        return std::nullopt;
      }
    }
  }

  bool skipValue() {
    skipSpace();
    if (pos_ >= input_.size()) {
      return false;
    }
    if (input_[pos_] == '"') {
      return parseString().has_value();
    }
    if (input_[pos_] == '{') {
      return skipObject();
    }
    if (input_[pos_] == '[') {
      return skipArray();
    }
    return skipScalar();
  }

  bool skipObject() {
    if (!consume('{')) {
      return false;
    }
    skipSpace();
    if (consume('}')) {
      return true;
    }
    while (true) {
      if (!parseString()) {
        return false;
      }
      if (!consume(':') || !skipValue()) {
        return false;
      }
      skipSpace();
      if (consume('}')) {
        return true;
      }
      if (!consume(',')) {
        return false;
      }
    }
  }

  bool skipArray() {
    if (!consume('[')) {
      return false;
    }
    skipSpace();
    if (consume(']')) {
      return true;
    }
    while (true) {
      if (!skipValue()) {
        return false;
      }
      skipSpace();
      if (consume(']')) {
        return true;
      }
      if (!consume(',')) {
        return false;
      }
    }
  }

  bool skipScalar() {
    skipSpace();
    const std::size_t start = pos_;
    while (pos_ < input_.size()) {
      const char ch = input_[pos_];
      if (std::isspace(static_cast<unsigned char>(ch)) != 0 || ch == ',' || ch == ']' ||
          ch == '}') {
        break;
      }
      ++pos_;
    }
    return pos_ > start;
  }

  static void normalize(RecognitionPayload *payload) {
    if (payload->commit_text.empty()) {
      if (!payload->candidates.empty()) {
        payload->commit_text = payload->candidates.front().text;
      }
    } else if (payload->candidates.empty()) {
      payload->candidates.push_back(
          Candidate{payload->commit_text, CandidateSource::Raw});
    }
  }

  std::string_view input_;
  std::size_t pos_ = 0;
};

} // namespace

std::string_view ToWireString(CandidateSource source) {
  switch (source) {
  case CandidateSource::Raw:
    return "raw";
  case CandidateSource::Llm:
    return "llm";
  case CandidateSource::Asr:
    return "asr";
  case CandidateSource::Cancel:
    return "cancel";
  }
  return "raw";
}

CandidateSource CandidateSourceFromWire(std::string_view source) {
  if (source == "llm") {
    return CandidateSource::Llm;
  }
  if (source == "asr") {
    return CandidateSource::Asr;
  }
  if (source == "cancel") {
    return CandidateSource::Cancel;
  }
  return CandidateSource::Raw;
}

RecognitionPayload ParseRecognitionPayload(std::string_view json) {
  return JsonCursor(json).parsePayload();
}

bool ShouldShowCandidateMenu(const RecognitionPayload &payload) {
  int llm_count = 0;
  for (const auto &candidate : payload.candidates) {
    if (candidate.source == CandidateSource::Llm) {
      ++llm_count;
    }
  }
  return llm_count > 1;
}

CommitPlan MakeCommitPlan(std::string_view json) {
  auto payload = ParseRecognitionPayload(json);
  return CommitPlan{payload, ShouldShowCandidateMenu(payload)};
}

} // namespace vinput_fcitx_bridge

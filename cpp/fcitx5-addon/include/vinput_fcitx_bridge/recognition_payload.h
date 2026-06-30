#pragma once

#include <string>
#include <string_view>
#include <vector>

namespace vinput_fcitx_bridge {

enum class CandidateSource { Raw, Llm, Asr, Cancel };

struct Candidate {
  std::string text;
  CandidateSource source = CandidateSource::Raw;
};

struct RecognitionPayload {
  std::string commit_text;
  std::vector<Candidate> candidates;
};

struct CommitPlan {
  RecognitionPayload payload;
  bool show_candidate_menu = false;
};

std::string_view ToWireString(CandidateSource source);
CandidateSource CandidateSourceFromWire(std::string_view source);
RecognitionPayload ParseRecognitionPayload(std::string_view json);
bool ShouldShowCandidateMenu(const RecognitionPayload &payload);
CommitPlan MakeCommitPlan(std::string_view json);

} // namespace vinput_fcitx_bridge

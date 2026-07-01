#include "vinput_fcitx_bridge/recognition_payload.h"

#include <cassert>
#include <string>

using vinput_fcitx_bridge::CandidateSource;
using vinput_fcitx_bridge::MakeCommitPlan;
using vinput_fcitx_bridge::ParseRecognitionPayload;
using vinput_fcitx_bridge::ShouldShowCandidateMenu;
using vinput_fcitx_bridge::ToWireString;

int main() {
  {
    const auto payload = ParseRecognitionPayload(
        R"({"commit_text":"hello","candidates":[{"text":"hello","source":"raw"}]})");
    assert(payload.commit_text == "hello");
    assert(payload.candidates.size() == 1);
    assert(payload.candidates[0].text == "hello");
    assert(payload.candidates[0].source == CandidateSource::Raw);
    assert(!ShouldShowCandidateMenu(payload));
  }

  {
    const auto payload =
        ParseRecognitionPayload(R"({"candidates":[{"text":"first","source":"asr"}]})");
    assert(payload.commit_text == "first");
    assert(payload.candidates.size() == 1);
    assert(payload.candidates[0].source == CandidateSource::Asr);
  }

  {
    const auto payload = ParseRecognitionPayload(
        R"({"commit_text":"line\n\u4F60\u597D","candidates":[]})");
    assert(payload.commit_text == std::string("line\n你好"));
    assert(payload.candidates.size() == 1);
    assert(payload.candidates[0].source == CandidateSource::Raw);
    assert(payload.candidates[0].text == std::string("line\n你好"));
  }

  {
    const auto plan = MakeCommitPlan(
        R"({"commit_text":"polished 1","candidates":[{"text":"raw transcript","source":"raw"},{"text":"polished 1","source":"llm"},{"text":"polished 2","source":"llm"}]})");
    assert(plan.payload.commit_text == "polished 1");
    assert(plan.show_candidate_menu);
  }

  {
    const auto plan = MakeCommitPlan(
        R"({"commit_text":"polished","candidates":[{"text":"raw transcript","source":"raw"},{"text":"asr command","source":"asr"},{"text":"polished","source":"llm"}]})");
    assert(plan.payload.commit_text == "polished");
    assert(plan.payload.candidates.size() == 3);
    assert(!plan.show_candidate_menu);
  }

  {
    const auto payload = ParseRecognitionPayload(
        R"({"commit_text":"fallback","candidates":[{"text":"fallback","source":"future"}]})");
    assert(payload.candidates.size() == 1);
    assert(payload.candidates[0].source == CandidateSource::Raw);
  }

  {
    const auto plan =
        MakeCommitPlan(R"({"candidates":[{"text":"","source":"cancel"}]})");
    assert(plan.payload.commit_text.empty());
    assert(plan.payload.candidates.size() == 1);
    assert(plan.payload.candidates[0].source == CandidateSource::Cancel);
    assert(!plan.show_candidate_menu);
  }

  {
    const auto payload = ParseRecognitionPayload(
        R"({"commit_text":"kept","trace":{"id":"42","tags":["new",null,true]},"candidates":[{"text":"kept","source":"llm","rank":1,"metadata":{"stable":true},"tags":["new"]}],"extra":[{"ignored":null}]})");
    assert(payload.commit_text == "kept");
    assert(payload.candidates.size() == 1);
    assert(payload.candidates[0].text == "kept");
    assert(payload.candidates[0].source == CandidateSource::Llm);
  }

  {
    const auto payload = ParseRecognitionPayload(
        R"({"candidates":[{"text":"","source":"raw"},{"text":"kept","source":"asr"}]})");
    assert(payload.commit_text == "kept");
    assert(payload.candidates.size() == 1);
    assert(payload.candidates[0].text == "kept");
    assert(payload.candidates[0].source == CandidateSource::Asr);
  }

  {
    const auto payload = ParseRecognitionPayload("not json");
    assert(payload.commit_text.empty());
    assert(payload.candidates.empty());
  }

  assert(ToWireString(CandidateSource::Raw) == "raw");
  assert(ToWireString(CandidateSource::Llm) == "llm");
  assert(ToWireString(CandidateSource::Asr) == "asr");
  assert(ToWireString(CandidateSource::Cancel) == "cancel");
  return 0;
}

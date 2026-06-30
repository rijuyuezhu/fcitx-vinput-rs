#include "vinput_fcitx_bridge/sd_bus_daemon_client.h"

#include "vinput_fcitx_bridge/dbus_contract.h"

#include <systemd/sd-bus.h>

#include <cerrno>
#include <cstring>
#include <string>
#include <utility>

namespace vinput_fcitx_bridge {
namespace {

void SetSdBusError(std::string *error, std::string_view action, int result,
                   const sd_bus_error &bus_error) {
  if (error == nullptr) {
    return;
  }

  std::string message(action);
  message += ": ";
  if (bus_error.message != nullptr) {
    message += bus_error.message;
  } else if (result < 0) {
    message += std::strerror(-result);
  } else {
    message += "unknown sd-bus error";
  }

  if (bus_error.name != nullptr) {
    message += " [";
    message += bus_error.name;
    message += ']';
  }
  *error = std::move(message);
}

void UnrefMessage(sd_bus_message *message) {
  if (message != nullptr) {
    sd_bus_message_unref(message);
  }
}

bool CallMethod(sd_bus *bus, std::string_view method, const char *signature,
                const char *argument, sd_bus_message **reply, std::string *error) {
  const auto bus_name = std::string(dbus::kServiceBusName);
  const auto object_path = std::string(dbus::kServiceObjectPath);
  const auto interface = std::string(dbus::kServiceInterface);
  const auto method_name = std::string(method);

  sd_bus_error bus_error = SD_BUS_ERROR_NULL;
  const int result =
      sd_bus_call_method(bus, bus_name.c_str(), object_path.c_str(), interface.c_str(),
                         method_name.c_str(), &bus_error, reply, signature, argument);
  if (result < 0) {
    SetSdBusError(error, method, result, bus_error);
    sd_bus_error_free(&bus_error);
    return false;
  }
  sd_bus_error_free(&bus_error);
  return true;
}

} // namespace

std::unique_ptr<SdBusDaemonClient>
SdBusDaemonClient::ConnectSession(std::string *error) {
  sd_bus *bus = nullptr;
  const int result = sd_bus_open_user(&bus);
  if (result < 0) {
    sd_bus_error bus_error = SD_BUS_ERROR_NULL;
    SetSdBusError(error, "connect user bus", result, bus_error);
    return nullptr;
  }
  return std::unique_ptr<SdBusDaemonClient>(new SdBusDaemonClient(bus));
}

SdBusDaemonClient::SdBusDaemonClient(sd_bus *bus) : bus_(bus) {}

SdBusDaemonClient::~SdBusDaemonClient() {
  if (bus_ != nullptr) {
    sd_bus_unref(bus_);
  }
}

bool SdBusDaemonClient::StartRecording(std::string *error) {
  return CallNoReply(dbus::kMethodStartRecording, error);
}

bool SdBusDaemonClient::StartCommandRecording(std::string_view selected_text,
                                              std::string *error) {
  return CallNoReplyWithString(dbus::kMethodStartCommandRecording, selected_text,
                               error);
}

bool SdBusDaemonClient::StopRecording(std::string_view scene_id,
                                      std::string *payload_json, std::string *error) {
  return CallStringReplyWithString(dbus::kMethodStopRecording, scene_id, payload_json,
                                   error);
}

bool SdBusDaemonClient::CallNoReply(std::string_view method, std::string *error) {
  sd_bus_message *reply = nullptr;
  const bool ok = CallMethod(bus_, method, "", nullptr, &reply, error);
  UnrefMessage(reply);
  return ok;
}

bool SdBusDaemonClient::CallNoReplyWithString(std::string_view method,
                                              std::string_view value,
                                              std::string *error) {
  const auto argument = std::string(value);
  sd_bus_message *reply = nullptr;
  const bool ok = CallMethod(bus_, method, "s", argument.c_str(), &reply, error);
  UnrefMessage(reply);
  return ok;
}

bool SdBusDaemonClient::CallStringReplyWithString(std::string_view method,
                                                  std::string_view value,
                                                  std::string *reply,
                                                  std::string *error) {
  const auto argument = std::string(value);
  sd_bus_message *message = nullptr;
  if (!CallMethod(bus_, method, "s", argument.c_str(), &message, error)) {
    return false;
  }

  const char *wire_reply = nullptr;
  const int result = sd_bus_message_read(message, "s", &wire_reply);
  if (result < 0) {
    sd_bus_error bus_error = SD_BUS_ERROR_NULL;
    SetSdBusError(error, "read string reply", result, bus_error);
    UnrefMessage(message);
    return false;
  }

  if (reply != nullptr) {
    *reply = wire_reply != nullptr ? wire_reply : "";
  }
  UnrefMessage(message);
  return true;
}

} // namespace vinput_fcitx_bridge

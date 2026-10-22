// Desktop build of the panel firmware: the real LVGL screen in an 800x480
// window, connected to the PC app over local TCP instead of USB serial.
//
//   panel_sim [host] [port]     (default 127.0.0.1 47800)

#define SDL_MAIN_HANDLED
#include <SDL.h>

#include <cstdio>
#include <cstdlib>
#include <string>

#include "lvgl.h"
#include "../src/mixer_ui.h"
#include "../src/panel_protocol.h"

#ifdef _WIN32
#include <winsock2.h>
#include <ws2tcpip.h>
using socket_t = SOCKET;
constexpr socket_t kNoSocket = INVALID_SOCKET;
#define close_socket closesocket
#else
#include <arpa/inet.h>
#include <fcntl.h>
#include <netinet/in.h>
#include <netinet/tcp.h>
#include <sys/socket.h>
#include <unistd.h>
using socket_t = int;
constexpr socket_t kNoSocket = -1;
#define close_socket close
#endif

namespace {

/// One TCP connection to the PC app, reconnecting every second while down.
class Link {
public:
    Link(std::string host, int port) : host_(std::move(host)), port_(port) {}

    bool connected() const { return sock_ != kNoSocket; }

    /// Connects if needed and returns complete lines received since last call.
    template <typename OnLine>
    void poll(OnLine on_line) {
        if (!connected()) {
            if (SDL_GetTicks() < retry_at_) return;
            retry_at_ = SDL_GetTicks() + 1000;
            open();
            if (!connected()) return;
        }
        char chunk[4096];
        for (;;) {
            int n = recv(sock_, chunk, sizeof chunk, 0);
            if (n > 0) {
                buffer_.append(chunk, n);
                continue;
            }
            if (n == 0 || !would_block()) drop();
            break;
        }
        size_t start = 0;
        for (size_t nl; (nl = buffer_.find('\n', start)) != std::string::npos; start = nl + 1) {
            std::string line = buffer_.substr(start, nl - start);
            if (!line.empty() && line.back() == '\r') line.pop_back();
            if (!line.empty()) on_line(line);
        }
        buffer_.erase(0, start);
    }

    void send_line(const std::string& line) {
        if (!connected()) return;
        std::string out = line + "\n";
        if (::send(sock_, out.data(), static_cast<int>(out.size()), 0) < 0 && !would_block()) drop();
    }

private:
    void open() {
        socket_t s = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
        if (s == kNoSocket) return;
        sockaddr_in addr{};
        addr.sin_family = AF_INET;
        addr.sin_port = htons(static_cast<uint16_t>(port_));
        inet_pton(AF_INET, host_.c_str(), &addr.sin_addr);
        if (connect(s, reinterpret_cast<sockaddr*>(&addr), sizeof addr) != 0) {
            close_socket(s);
            return;
        }
        int one = 1;
        setsockopt(s, IPPROTO_TCP, TCP_NODELAY, reinterpret_cast<const char*>(&one), sizeof one);
#ifdef _WIN32
        u_long nonblocking = 1;
        ioctlsocket(s, FIONBIO, &nonblocking);
#else
        fcntl(s, F_SETFL, fcntl(s, F_GETFL, 0) | O_NONBLOCK);
#endif
        sock_ = s;
        buffer_.clear();
        std::printf("connected to %s:%d\n", host_.c_str(), port_);
    }

    void drop() {
        close_socket(sock_);
        sock_ = kNoSocket;
        std::printf("disconnected\n");
    }

    static bool would_block() {
#ifdef _WIN32
        return WSAGetLastError() == WSAEWOULDBLOCK;
#else
        return errno == EWOULDBLOCK || errno == EAGAIN;
#endif
    }

    std::string host_;
    int port_;
    socket_t sock_ = kNoSocket;
    std::string buffer_;
    uint32_t retry_at_ = 0;
};

Link* link = nullptr;
panel::Model model;

void on_touch_mute(int slot) {
    if (link) link->send_line(panel::touch_mute_line(slot));
}

uint32_t ticks() { return SDL_GetTicks(); }

}  // namespace

int main(int argc, char** argv) {
#ifdef _WIN32
    WSADATA wsa;
    WSAStartup(MAKEWORD(2, 2), &wsa);
#endif
    std::string host = argc > 1 ? argv[1] : "127.0.0.1";
    int port = argc > 2 ? std::atoi(argv[2]) : 47800;

    lv_init();
    lv_tick_set_cb(ticks);
    lv_display_t* display = lv_sdl_window_create(800, 480);
    lv_sdl_window_set_title(display, "Skjerm-simulator - DIY Sound Mixer");
    lv_sdl_mouse_create();

    ui::init(on_touch_mute);

    Link tcp(host, port);
    link = &tcp;
    bool was_connected = false;

    for (;;) {
        tcp.poll([](const std::string& line) {
            panel::Update update = panel::handle_line(line, model);
            for (const std::string& reply : update.replies) link->send_line(reply);
            ui::apply(model, update);
        });
        if (tcp.connected() != was_connected) {
            was_connected = tcp.connected();
            if (was_connected) {
                // Introduce ourselves right away; the PC also asks with hello?.
                tcp.send_line(panel::hello_line());
            } else {
                model = panel::Model{};
            }
            ui::set_connected(was_connected);
        }
        uint32_t wait = lv_timer_handler();
        SDL_Delay(wait < 5 ? wait : 5);
    }
}

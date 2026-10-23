import base64
from pathlib import Path
import unittest
from unittest.mock import patch
import network


class RecoveryTests(unittest.TestCase):
    def state(self):
        return {'routes': network.route_plan('192.0.2.20', 'eth0', '192.0.2.1'),
                'device': True, 'children': [{'pid': 123, 'token': ['old', '1']}],
                'resolv_backup': base64.b64encode(b'nameserver 192.0.2.53\n').decode()}

    def test_restores_owned_dns_and_only_scoped_routes(self):
        with patch.object(network, 'command') as cmd, patch.object(network, 'process_token', return_value=['different', '2']), \
             patch.object(network.os, 'kill') as kill, patch.object(Path, 'read_bytes', return_value=network.DNS_BYTES), \
             patch.object(Path, 'write_bytes') as write:
            self.assertTrue(network.recover(self.state()))
            kill.assert_not_called()
            write.assert_called_once_with(b'nameserver 192.0.2.53\n')
            routes = [x.args[0] for x in cmd.call_args_list if 'route' in x.args[0]]
            self.assertEqual(len(routes), 5)
            for route in routes:
                self.assertIn('del', route)
                self.assertIn(network.METRIC, route)
                self.assertIn('99', route)
            self.assertIn(unittest.mock.call(['ip', 'link', 'delete', network.DEVICE], check=False), cmd.call_args_list)

    def test_preserves_external_dns_change_but_removes_interface(self):
        with patch.object(network, 'command') as cmd, patch.object(network, 'process_token', return_value=None), \
             patch.object(Path, 'read_bytes', return_value=b'nameserver 192.0.2.99\n'), \
             patch.object(Path, 'write_bytes') as write:
            self.assertFalse(network.recover(self.state()))
            write.assert_not_called()
            cmd.assert_any_call(['ip', 'link', 'delete', network.DEVICE], check=False)

    def test_server_exclusion_precedes_capture_and_does_not_delete_default(self):
        routes = network.route_plan('198.51.100.10', 'wlan0', '192.168.1.1')
        self.assertEqual(routes[0][3], '198.51.100.10/32')
        self.assertIn('wlan0', routes[0])
        self.assertTrue(all('default' not in r and 'replace' not in r for r in routes))
        self.assertEqual(sum('unreachable' in r for r in routes), 2)


if __name__ == '__main__':
    unittest.main()


class SupervisionTests(unittest.TestCase):
    def test_retries_are_bounded_and_exponential(self):
        with patch.object(network, 'network_available', return_value=True), patch.object(network, 'remove_guard'), patch.object(network, 'STOP') as stop, patch.object(network, 'backend', side_effect=network.RecoverableError('offline')) as backend, \
             patch.object(network.threading, 'Thread'), patch.object(network, 'log'), patch.object(network.secrets, 'randbelow', return_value=0):
            stop.is_set.return_value = False
            stop.wait.return_value = False
            with self.assertRaisesRegex(RuntimeError, '5 attempts'):
                network.supervise(['same-profile'])
            self.assertEqual(backend.call_count, 6)
            self.assertEqual([c.args[0] for c in stop.wait.call_args_list], [1, 2, 4, 8, 15])
            self.assertTrue(all(c.args == (['same-profile'],) for c in backend.call_args_list))

    def test_manual_stop_interrupts_backoff(self):
        with patch.object(network, 'network_available', return_value=True), patch.object(network, 'remove_guard'), patch.object(network, 'STOP') as stop, patch.object(network, 'backend', side_effect=network.RecoverableError('offline')) as backend, \
             patch.object(network.threading, 'Thread'), patch.object(network, 'log'), patch.object(network.secrets, 'randbelow', return_value=0):
            stop.is_set.return_value = False
            stop.wait.return_value = True
            network.supervise([])
            self.assertEqual(backend.call_count, 1)

    def test_configuration_errors_are_not_retried(self):
        with patch.object(network, 'network_available', return_value=True), patch.object(network, 'remove_guard'), patch.object(network, 'STOP') as stop, patch.object(network, 'backend', side_effect=RuntimeError('invalid core config')) as backend, \
             patch.object(network.threading, 'Thread'):
            stop.is_set.return_value = False
            with self.assertRaisesRegex(RuntimeError, 'invalid core config'): network.supervise([])
            self.assertEqual(backend.call_count, 1)


class HealthVerificationTests(unittest.TestCase):
    def routes(self, args):
        if 'link' in args:
            return [{'flags': ['UP']}]
        if '-6' in args:
            return [{'dst': x, 'type': 'unreachable', 'metric': 51863}
                    for x in ['::/1', '8000::/1']]
        return [{'dev': 'sni0'}]

    def test_healthy_route_checks(self):
        with patch.object(network, 'read_json', side_effect=self.routes):
            network.verify_routes()

    def test_missing_adapter_fails_before_probe(self):
        with patch.object(network, 'read_json', return_value=[]), patch.object(network, 'https_probe') as probe:
            with self.assertRaisesRegex(RuntimeError, 'adapter'):
                network.verify(1234)
            probe.assert_not_called()

    def test_direct_ipv4_route_is_rejected(self):
        def changed(args):
            return [{'dev': 'wlan0'}] if 'get' in args else self.routes(args)
        with patch.object(network, 'read_json', side_effect=changed):
            with self.assertRaisesRegex(RuntimeError, 'bypasses'):
                network.verify_routes()

    def test_missing_ipv6_protection_is_rejected(self):
        def changed(args):
            return [] if '-6' in args else self.routes(args)
        with patch.object(network, 'read_json', side_effect=changed):
            with self.assertRaisesRegex(RuntimeError, 'IPv6'):
                network.verify_routes()

    def test_proxy_success_does_not_hide_system_failure(self):
        with patch.object(network, 'verify_guard', return_value=False), patch.object(network, 'verify_routes'), patch.object(network, 'read_json', return_value=[{'dev': 'sni0'}]), \
             patch.object(network.socket, 'getaddrinfo', return_value=[(2, 1, 6, '', ('1.1.1.1', 443))]), \
             patch.object(network, 'https_probe', side_effect=['198.51.100.1', '192.0.2.1'] * 2):
            with self.assertRaisesRegex(RuntimeError, 'exits differ'):
                network.verify(1234)

    def test_fragmented_socks_reply_is_read_completely(self):
        from unittest.mock import Mock
        sock = Mock()
        sock.recv.side_effect = [b'\x05', b'\x00']
        self.assertEqual(network.receive_exact(sock, 2), b'\x05\x00')

    def test_truncated_socks_reply_fails(self):
        from unittest.mock import Mock
        sock = Mock()
        sock.recv.side_effect = [b'\x05', b'']
        with self.assertRaisesRegex(RuntimeError, 'closed'):
            network.receive_exact(sock, 2)

class DnsAnswerTests(unittest.TestCase):
    def query(self):
        return b'\x12\x34\x01\x00\x00\x01\x00\x00\x00\x00\x00\x00\x07example\x03com\x00\x00\x01\x00\x01'

    def answer(self):
        answer = bytearray(self.query())
        answer[2:4] = b'\x81\x80'
        answer[7] = 1
        return bytes(answer) + b'\xc0\x0c\x00\x01\x00\x01\x00\x00\x00\x3c\x00\x04\x5d\xb8\xd8\x22'

    def test_complete_a_record_is_accepted(self):
        self.assertTrue(network.dns_answer_valid(self.answer(), self.query()))

    def test_claimed_but_missing_answer_is_rejected(self):
        self.assertFalse(network.dns_answer_valid(self.answer()[:-4], self.query()))

    def test_wrong_transaction_is_rejected(self):
        self.assertFalse(network.dns_answer_valid(b'\x00\x00' + self.answer()[2:], self.query()))

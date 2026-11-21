package bit.kedybaykedy.northbridge.transport

import java.net.Socket
import java.security.KeyStore
import java.security.Principal
import java.security.cert.Certificate
import java.security.cert.CertificateException
import java.security.cert.X509Certificate
import javax.net.ssl.KeyManager
import javax.net.ssl.KeyManagerFactory
import javax.net.ssl.SSLContext
import javax.net.ssl.SSLServerSocketFactory
import javax.net.ssl.SSLSocketFactory
import javax.net.ssl.TrustManager
import javax.net.ssl.X509KeyManager
import javax.net.ssl.X509TrustManager

object Tls {

    fun trustAll(): X509TrustManager = object : X509TrustManager {
        override fun checkClientTrusted(chain: Array<out X509Certificate>?, authType: String?) = Unit

        override fun checkServerTrusted(chain: Array<out X509Certificate>?, authType: String?) = Unit

        override fun getAcceptedIssuers(): Array<X509Certificate> = arrayOf()
    }

    fun pinned(expectedFingerprint: String): X509TrustManager {
        val expected = expectedFingerprint.uppercase()
        return object : X509TrustManager {
            override fun checkClientTrusted(chain: Array<out X509Certificate>?, authType: String?) = Unit

            override fun checkServerTrusted(chain: Array<out X509Certificate>?, authType: String?) {
                val certificate = chain?.firstOrNull()
                    ?: throw CertificateException("peer presented no certificate")
                val actual = sha256Upper(certificate.encoded)
                if (actual != expected) {
                    throw CertificateException("server fingerprint mismatch: expected $expected, got $actual")
                }
            }

            override fun getAcceptedIssuers(): Array<X509Certificate> = arrayOf()
        }
    }

    private fun issuerBlindKeyManager(delegate: X509KeyManager, alias: String): X509KeyManager =
        object : X509KeyManager {
            override fun chooseClientAlias(
                keyType: Array<out String>?,
                issuers: Array<out Principal>?,
                socket: Socket?,
            ): String = alias

            override fun chooseServerAlias(
                keyType: String?,
                issuers: Array<out Principal>?,
                socket: Socket?,
            ): String? = delegate.chooseServerAlias(keyType, issuers, socket)

            override fun getCertificateChain(alias: String?): Array<X509Certificate>? =
                delegate.getCertificateChain(alias)

            override fun getPrivateKey(alias: String?): java.security.PrivateKey? =
                delegate.getPrivateKey(alias)

            override fun getClientAliases(keyType: String?, issuers: Array<out Principal>?): Array<String>? =
                delegate.getClientAliases(keyType, issuers)

            override fun getServerAliases(keyType: String?, issuers: Array<out Principal>?): Array<String>? =
                delegate.getServerAliases(keyType, issuers)
        }

    fun trustingFingerprints(fingerprints: () -> Set<String>): X509TrustManager {
        return object : X509TrustManager {
            override fun checkClientTrusted(chain: Array<out X509Certificate>?, authType: String?) {
                val certificate = chain?.firstOrNull()
                    ?: throw CertificateException("peer presented no certificate")
                val actual = sha256Upper(certificate.encoded)
                if (!fingerprints().map { it.uppercase() }.contains(actual)) {
                    throw CertificateException("unknown peer fingerprint: $actual")
                }
            }

            override fun checkServerTrusted(chain: Array<out X509Certificate>?, authType: String?) = Unit

            override fun getAcceptedIssuers(): Array<X509Certificate> = arrayOf()
        }
    }

    fun socketFactory(identity: PhoneIdentity, trust: X509TrustManager): SSLSocketFactory {
        val context = serverContext(identity, trust)
        return context.socketFactory
    }

    fun serverSocketFactory(identity: PhoneIdentity, trust: X509TrustManager): SSLServerSocketFactory {
        val context = serverContext(identity, trust)
        return context.serverSocketFactory
    }

    private fun serverContext(identity: PhoneIdentity, trust: X509TrustManager): SSLContext {
        val keyStore = KeyStore.getInstance("PKCS12")
        keyStore.load(null, null)
        keyStore.setKeyEntry("nb", identity.privateKey, charArrayOf(), arrayOf<Certificate>(identity.cert))
        val keyManagerFactory = KeyManagerFactory.getInstance(KeyManagerFactory.getDefaultAlgorithm())
        keyManagerFactory.init(keyStore, charArrayOf())
        val keyManagers = keyManagerFactory.keyManagers.map { manager ->
            if (manager is X509KeyManager) issuerBlindKeyManager(manager, "nb") else manager
        }
        val context = SSLContext.getInstance("TLS")
        context.init(keyManagers.toTypedArray(), arrayOf<TrustManager>(trust), null)
        return context
    }
}

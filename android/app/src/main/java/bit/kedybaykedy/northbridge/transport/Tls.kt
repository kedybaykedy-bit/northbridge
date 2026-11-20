package bit.kedybaykedy.northbridge.transport

import java.security.KeyStore
import java.security.cert.Certificate
import java.security.cert.CertificateException
import java.security.cert.X509Certificate
import javax.net.ssl.KeyManagerFactory
import javax.net.ssl.SSLContext
import javax.net.ssl.SSLSocketFactory
import javax.net.ssl.TrustManager
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

    fun socketFactory(identity: PhoneIdentity, trust: X509TrustManager): SSLSocketFactory {
        val keyStore = KeyStore.getInstance("PKCS12")
        keyStore.load(null, null)
        keyStore.setKeyEntry("nb", identity.privateKey, charArrayOf(), arrayOf<Certificate>(identity.cert))
        val keyManagerFactory = KeyManagerFactory.getInstance(KeyManagerFactory.getDefaultAlgorithm())
        keyManagerFactory.init(keyStore, charArrayOf())
        val context = SSLContext.getInstance("TLS")
        context.init(keyManagerFactory.keyManagers, arrayOf<TrustManager>(trust), null)
        return context.socketFactory
    }
}

import axios from 'axios';
import env from '@/swap/config/env';
import screenAddress from '../screenAddress';

jest.mock('axios');

describe(screenAddress, () => {
  it('returns true if sanctions are found', async () => {
    env.CHAINALYSIS_API_KEY = 'test';
    jest.mocked(axios.get).mockResolvedValue({
      data: {
        identifications: [
          {
            category: 'sanctions',
            name: 'SANCTIONS: OFAC SDN Secondeye Solution 2021-04-15 72a5843cc08275c8171e582972aa4fda8c397b2a',
            description:
              'Pakistan-based Secondeye Solution (SES), also known as Forwarderz, is a synthetic identity document vendor that was added to the OFAC SDN list in April 2021.\n' +
              '\n' +
              'SES customers could buy fake identity documents to sign up for accounts with cryptocurrency exchanges, payment providers, banks, and more under false identities. According to the US Treasury Department, SES assisted the Internet Research Agency (IRA), the Russian troll farm that OFAC designated pursuant to E.O. 13848 in 2018 for interfering in the 2016 presidential election, in concealing its identity to evade sanctions.\n' +
              '\n' +
              'https://home.treasury.gov/news/press-releases/jy0126',
            url: 'https://home.treasury.gov/news/press-releases/jy0126',
          },
        ],
      },
    });

    const result = await screenAddress(
      '0x72a5843cc08275C8171E582972Aa4fDa8C397B2A',
    );

    expect(result).toBe(true);
  });

  it('returns false if no API key is present', async () => {
    env.CHAINALYSIS_API_KEY = undefined;
    jest.mocked(axios.get).mockResolvedValue({
      data: { identifications: [] },
    });

    const result = await screenAddress(
      '0x72a5843cc08275C8171E582972Aa4fDa8C397B2A',
    );

    expect(result).toBe(false);
  });

  it('returns false if parsing fails', async () => {
    env.CHAINALYSIS_API_KEY = 'test';
    jest.mocked(axios.get).mockResolvedValue({
      data: { ids: [] },
    });

    const result = await screenAddress(
      '0x72a5843cc08275C8171E582972Aa4fDa8C397B2A',
    );

    expect(result).toBe(false);
  });

  it('returns false if no sanctions are found', async () => {
    env.CHAINALYSIS_API_KEY = 'test';
    jest.mocked(axios.get).mockResolvedValue({
      data: { identifications: [] },
    });

    const result = await screenAddress(
      '0x72a5843cc08275C8171E582972Aa4fDa8C397B2A',
    );

    expect(result).toBe(false);
  });

  it('returns false if the API call throws', async () => {
    env.CHAINALYSIS_API_KEY = 'test';
    jest.mocked(axios.get).mockRejectedValueOnce(new Error('test'));

    const result = await screenAddress(
      '0x72a5843cc08275C8171E582972Aa4fDa8C397B2A',
    );

    expect(result).toBe(false);
  });
});
